use crate::router::state::{AppState, NodeStore};
use axum::extract::{Json as ExtractJson, Query, State};
use axum::{
    Json, Router,
    routing::{get, post},
};
use common::{Machine, MachineId, MachineState, Node, NodeId};
use rand::seq::IteratorRandom;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
struct SetParams {
    machine_name: String,
    command: String,
    node_id: Option<NodeId>,
}

#[derive(Debug, Deserialize)]
struct StopParams {
    machine_id: MachineId,
}

#[derive(Debug, Deserialize)]
struct ShowParams {
    machine_id: Option<MachineId>,
}

#[derive(Debug, Deserialize)]
struct LaunchMachineRequest {
    machine_name: String,
    command: String,
    node_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkerMachinesParams {
    node_id: NodeId,
}

#[derive(Debug, Deserialize)]
struct MachineClaimRequest {
    node_id: NodeId,
    machine_id: MachineId,
}

#[derive(Debug, Deserialize)]
struct MachineReportRequest {
    node_id: NodeId,
    machine_id: MachineId,
    state: MachineState,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Serialize)]
struct MachineLaunchResponse {
    machine_id: String,
    machine_name: String,
    node_id: String,
    state: MachineState,
    command: String,
}

#[derive(Debug, Serialize)]
struct MachineAssignment {
    machine_id: String,
    machine_name: String,
    command: String,
}

#[derive(Debug, Serialize)]
struct MachineMutationResponse {
    machine_id: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct MachineSummary {
    machine_id: String,
    machine_name: String,
    node_id: String,
    state: MachineState,
    command: String,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

async fn pick_random_node(state: &NodeStore) -> Option<Node> {
    let nodes = state.lock().await;
    let mut rng = rand::thread_rng();
    nodes
        .values()
        .filter(|node| node.is_schedulable())
        .choose(&mut rng)
        .cloned()
}

async fn pick_node(state: &NodeStore, requested: Option<&NodeId>) -> Option<Node> {
    match requested {
        Some(node_id) => {
            let nodes = state.lock().await;
            nodes
                .get(node_id)
                .filter(|node| node.is_schedulable())
                .cloned()
        }
        None => pick_random_node(state).await,
    }
}

async fn launch_conflict_message(
    state: &NodeStore,
    requested: Option<&NodeId>,
    machine_name: &str,
) -> String {
    let nodes = state.lock().await;

    match requested {
        Some(node_id) => match nodes.get(node_id) {
            Some(node) => {
                let mut reasons = Vec::new();

                if node.observed_state != common::NodeState::Running {
                    reasons.push(format!("observed_state={:?}", node.observed_state));
                }
                if node.desired_state != common::NodeState::Running {
                    reasons.push(format!("desired_state={:?}", node.desired_state));
                }
                if !node.supports_machine_execution {
                    reasons.push("execution=legacy".to_string());
                }
                if node.cordoned {
                    reasons.push("cordoned=true".to_string());
                }
                if node.draining {
                    reasons.push("draining=true".to_string());
                }

                if reasons.is_empty() {
                    format!(
                        "Node '{}' is unavailable for machine placement right now.",
                        node_id
                    )
                } else {
                    format!(
                        "Node '{}' cannot run machines right now: {}.",
                        node_id,
                        reasons.join(", ")
                    )
                }
            }
            None => format!("Node '{}' was not found.", node_id),
        },
        None => {
            let total = nodes.len();
            let ready = nodes.values().filter(|node| node.is_schedulable()).count();
            let legacy = nodes
                .values()
                .filter(|node| !node.supports_machine_execution)
                .count();
            let pending = nodes
                .values()
                .filter(|node| node.observed_state != common::NodeState::Running)
                .count();
            let blocked = nodes
                .values()
                .filter(|node| node.cordoned || node.draining)
                .count();

            if total == 0 {
                format!(
                    "No nodes are registered yet, so machine '{}' has nowhere to run.",
                    machine_name
                )
            } else {
                format!(
                    "No schedulable nodes are available for machine '{}'. total_nodes={}, ready={}, legacy={}, not_running={}, blocked={}.",
                    machine_name, total, ready, legacy, pending, blocked
                )
            }
        }
    }
}

async fn launch_handler(
    State(state): State<AppState>,
    Query(params): Query<SetParams>,
) -> Result<Json<MachineLaunchResponse>, (axum::http::StatusCode, String)> {
    let command = params.command.trim();

    if command.is_empty() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "Machine command must not be empty".to_string(),
        ));
    }

    let node = match pick_node(&state.nodes, params.node_id.as_ref()).await {
        Some(node) => node,
        None => {
            let message = launch_conflict_message(
                &state.nodes,
                params.node_id.as_ref(),
                &params.machine_name,
            )
            .await;
            return Err((axum::http::StatusCode::CONFLICT, message));
        }
    };

    info!(
        "Assigned machine '{}' to node '{}' with command '{}'",
        params.machine_name, node.name, command
    );

    let machine_id = MachineId::new();
    let machine = Machine {
        node_id: node.id,
        id: machine_id.as_str().to_string(),
        name: params.machine_name.clone(),
        state: MachineState::Pending,
        command: command.to_string(),
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
    };

    let mut machines = state.machines.lock().await;

    let response = MachineLaunchResponse {
        machine_id: machine_id.as_str().to_string(),
        machine_name: machine.name.clone(),
        node_id: machine.node_id.as_str().to_string(),
        state: machine.state.clone(),
        command: machine.command.clone(),
    };

    machines.insert(machine_id.clone(), machine);

    Ok(Json(response))
}

async fn launch_from_json_handler(
    State(state): State<AppState>,
    ExtractJson(payload): ExtractJson<LaunchMachineRequest>,
) -> Result<Json<MachineLaunchResponse>, (axum::http::StatusCode, String)> {
    launch_handler(
        State(state),
        Query(SetParams {
            machine_name: payload.machine_name,
            command: payload.command,
            node_id: payload.node_id.map(NodeId::new),
        }),
    )
    .await
}

async fn stop_handler(
    State(state): State<AppState>,
    Query(params): Query<StopParams>,
) -> Json<MachineMutationResponse> {
    let mut map = state.machines.lock().await;

    match map.remove(&params.machine_id) {
        Some(machine) => {
            let machine_name = machine.name.clone();
            let machine_node_id = machine.node_id.clone();
            drop(map);

            let remaining = state
                .machines
                .lock()
                .await
                .values()
                .filter(|m| m.node_id == machine_node_id && m.state.is_active())
                .count();

            let mut nodes = state.nodes.lock().await;
            match nodes.get_mut(&machine_node_id) {
                Some(node) if node.draining && remaining == 0 => node.draining = false,
                _ => {}
            }

            Json(MachineMutationResponse {
                machine_id: params.machine_id.to_string(),
                message: format!(
                    "Removed machine: id={}, name={}",
                    params.machine_id, machine_name
                ),
            })
        }
        None => Json(MachineMutationResponse {
            machine_id: params.machine_id.to_string(),
            message: format!("No machine found with id={}", params.machine_id),
        }),
    }
}

async fn show_handler(State(state): State<AppState>, Query(params): Query<ShowParams>) -> String {
    let map = state.machines.lock().await;

    if map.is_empty() {
        return "No machines stored".to_string();
    }

    match params.machine_id.as_ref() {
        Some(machine_id_str) => {
            let machine_id = machine_id_str.clone();

            match map.get(&machine_id) {
                Some(machine) => format!(
                    "Machine found: id={}, name={}, node_id={}, state={:?}, command={}, exit_code={:?}\nstdout:\n{}\nstderr:\n{}",
                    machine_id,
                    machine.name,
                    machine.node_id,
                    machine.state,
                    machine.command,
                    machine.exit_code,
                    machine.stdout,
                    machine.stderr
                ),
                None => format!("No machine found with id={}", machine_id),
            }
        }
        None => {
            let mut lines = Vec::new();
            for (machine_id, machine) in map.iter() {
                lines.push(format!(
                    "id={}, name={}, node_id={}, state={:?}, command={}",
                    machine_id, machine.name, machine.node_id, machine.state, machine.command
                ));
            }
            lines.join("\n")
        }
    }
}

async fn list_machines_handler(State(state): State<AppState>) -> Json<Vec<MachineSummary>> {
    let mut machines = state
        .machines
        .lock()
        .await
        .values()
        .map(|machine| MachineSummary {
            machine_id: machine.id.clone(),
            machine_name: machine.name.clone(),
            node_id: machine.node_id.as_str().to_string(),
            state: machine.state.clone(),
            command: machine.command.clone(),
            exit_code: machine.exit_code,
            stdout: machine.stdout.clone(),
            stderr: machine.stderr.clone(),
        })
        .collect::<Vec<_>>();

    machines.sort_by(|left, right| left.machine_name.cmp(&right.machine_name));

    Json(machines)
}

async fn worker_assignments_handler(
    State(state): State<AppState>,
    Query(params): Query<WorkerMachinesParams>,
) -> Json<Vec<MachineAssignment>> {
    let machines = state.machines.lock().await;

    let assignments = machines
        .values()
        .filter(|machine| {
            machine.node_id == params.node_id && machine.state == MachineState::Pending
        })
        .map(|machine| MachineAssignment {
            machine_id: machine.id.clone(),
            machine_name: machine.name.clone(),
            command: machine.command.clone(),
        })
        .collect::<Vec<_>>();

    Json(assignments)
}

async fn worker_claim_handler(
    State(state): State<AppState>,
    ExtractJson(payload): ExtractJson<MachineClaimRequest>,
) -> Result<Json<MachineMutationResponse>, (axum::http::StatusCode, String)> {
    let mut machines = state.machines.lock().await;
    let Some(machine) = machines.get_mut(&payload.machine_id) else {
        return Err((
            axum::http::StatusCode::NOT_FOUND,
            format!("Machine '{}' not found", payload.machine_id),
        ));
    };

    if machine.node_id != payload.node_id {
        return Err((
            axum::http::StatusCode::CONFLICT,
            format!(
                "Machine '{}' is assigned to node '{}', not '{}'",
                payload.machine_id, machine.node_id, payload.node_id
            ),
        ));
    }

    if machine.state != MachineState::Pending {
        return Err((
            axum::http::StatusCode::CONFLICT,
            format!(
                "Machine '{}' is already in state {:?}",
                payload.machine_id, machine.state
            ),
        ));
    }

    machine.state = MachineState::Running;
    machine.exit_code = None;
    machine.stdout.clear();
    machine.stderr.clear();

    Ok(Json(MachineMutationResponse {
        machine_id: payload.machine_id.to_string(),
        message: format!("Machine '{}' claimed for execution", payload.machine_id),
    }))
}

async fn worker_report_handler(
    State(state): State<AppState>,
    ExtractJson(payload): ExtractJson<MachineReportRequest>,
) -> Result<Json<MachineMutationResponse>, (axum::http::StatusCode, String)> {
    let mut machines = state.machines.lock().await;
    let Some(machine) = machines.get_mut(&payload.machine_id) else {
        return Err((
            axum::http::StatusCode::NOT_FOUND,
            format!("Machine '{}' not found", payload.machine_id),
        ));
    };

    if machine.node_id != payload.node_id {
        warn!(
            "Node '{}' tried to report machine '{}' owned by '{}'",
            payload.node_id, payload.machine_id, machine.node_id
        );
        return Err((
            axum::http::StatusCode::CONFLICT,
            format!(
                "Machine '{}' is assigned to node '{}', not '{}'",
                payload.machine_id, machine.node_id, payload.node_id
            ),
        ));
    }

    machine.state = payload.state.clone();
    machine.exit_code = payload.exit_code;
    machine.stdout = payload.stdout;
    machine.stderr = payload.stderr;
    let machine_node_id = machine.node_id.clone();
    let machine_is_active = machine.state.is_active();
    drop(machines);

    if !machine_is_active {
        let remaining = state
            .machines
            .lock()
            .await
            .values()
            .filter(|m| m.node_id == machine_node_id)
            .filter(|m| m.state.is_active())
            .count();

        let mut nodes = state.nodes.lock().await;
        match nodes.get_mut(&machine_node_id) {
            Some(node) if node.draining && remaining == 0 => node.draining = false,
            _ => {}
        }
    }

    Ok(Json(MachineMutationResponse {
        machine_id: payload.machine_id.to_string(),
        message: format!(
            "Recorded machine '{}' result as {:?}",
            payload.machine_id, payload.state
        ),
    }))
}

pub fn machines_router() -> Router<AppState> {
    Router::new()
        .route("/api/machines", get(list_machines_handler))
        .route("/api/machines", post(launch_from_json_handler))
        .route("/machines/launch", post(launch_handler))
        .route("/machines/stop", post(stop_handler))
        .route("/machines/show", get(show_handler))
        .route("/workers/machines", get(worker_assignments_handler))
        .route("/workers/machines/claim", post(worker_claim_handler))
        .route("/workers/machines/report", post(worker_report_handler))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::NoopWorkerLauncher;
    use crate::router::state::AppState;
    use common::{MachineState, NodeId, NodeState};
    use std::sync::Arc;

    #[tokio::test]
    async fn launch_skips_unschedulable_nodes() {
        let state = AppState {
            app_port: 3000,
            machines: Default::default(),
            nodes: Default::default(),
            launched_workers: Default::default(),
            launcher: Arc::new(NoopWorkerLauncher),
        };
        let node_id = NodeId::new("n1");

        {
            let mut nodes = state.nodes.lock().await;
            nodes.insert(
                node_id.clone(),
                Node {
                    id: node_id,
                    name: "n1".into(),
                    observed_state: NodeState::Running,
                    desired_state: NodeState::Running,
                    supports_machine_execution: true,
                    cordoned: true,
                    draining: false,
                    last_heartbeat: tokio::time::Instant::now(),
                },
            );
        }

        let response = launch_handler(
            State(state),
            Query(SetParams {
                machine_name: "machine-a".into(),
                command: "echo hello".into(),
                node_id: None,
            }),
        )
        .await;

        let (status, message) = response.unwrap_err();
        assert_eq!(status, axum::http::StatusCode::CONFLICT);
        assert_eq!(
            message,
            "No schedulable nodes are available for machine 'machine-a'. total_nodes=1, ready=0, legacy=0, not_running=0, blocked=1."
        );
    }

    #[tokio::test]
    async fn launch_skips_nodes_without_execution_support() {
        let state = AppState {
            app_port: 3000,
            machines: Default::default(),
            nodes: Default::default(),
            launched_workers: Default::default(),
            launcher: Arc::new(NoopWorkerLauncher),
        };
        let node_id = NodeId::new("legacy-node");

        {
            let mut nodes = state.nodes.lock().await;
            nodes.insert(
                node_id.clone(),
                Node {
                    id: node_id,
                    name: "legacy-node".into(),
                    observed_state: NodeState::Running,
                    desired_state: NodeState::Running,
                    supports_machine_execution: false,
                    cordoned: false,
                    draining: false,
                    last_heartbeat: tokio::time::Instant::now(),
                },
            );
        }

        let response = launch_handler(
            State(state),
            Query(SetParams {
                machine_name: "machine-a".into(),
                command: "echo hello".into(),
                node_id: None,
            }),
        )
        .await;

        let (status, message) = response.unwrap_err();
        assert_eq!(status, axum::http::StatusCode::CONFLICT);
        assert_eq!(
            message,
            "No schedulable nodes are available for machine 'machine-a'. total_nodes=1, ready=0, legacy=1, not_running=0, blocked=0."
        );
    }

    #[tokio::test]
    async fn stop_completes_drain_when_last_machine_is_removed() {
        let state = AppState {
            app_port: 3000,
            machines: Default::default(),
            nodes: Default::default(),
            launched_workers: Default::default(),
            launcher: Arc::new(NoopWorkerLauncher),
        };
        let node_id = NodeId::new("n1");
        let machine_id = MachineId::new();

        {
            let mut nodes = state.nodes.lock().await;
            nodes.insert(
                node_id.clone(),
                Node {
                    id: node_id.clone(),
                    name: "n1".into(),
                    observed_state: NodeState::Running,
                    desired_state: NodeState::Running,
                    supports_machine_execution: true,
                    cordoned: true,
                    draining: true,
                    last_heartbeat: tokio::time::Instant::now(),
                },
            );
        }

        {
            let mut machines = state.machines.lock().await;
            machines.insert(
                machine_id.clone(),
                Machine {
                    id: machine_id.as_str().to_string(),
                    name: "machine-a".into(),
                    node_id: node_id.clone(),
                    state: MachineState::Running,
                    command: "echo hello".into(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                },
            );
        }

        let response = stop_handler(
            State(state.clone()),
            Query(StopParams {
                machine_id: machine_id.clone(),
            }),
        )
        .await;

        assert_eq!(
            response.message,
            format!("Removed machine: id={}, name={}", machine_id, "machine-a")
        );

        let nodes = state.nodes.lock().await;
        let node = nodes.get(&node_id).unwrap();
        assert!(node.cordoned);
        assert!(!node.draining);
    }

    #[tokio::test]
    async fn worker_report_updates_machine_output() {
        let state = AppState {
            app_port: 3000,
            machines: Default::default(),
            nodes: Default::default(),
            launched_workers: Default::default(),
            launcher: Arc::new(NoopWorkerLauncher),
        };
        let node_id = NodeId::new("n1");
        let machine_id = MachineId::new();

        {
            let mut machines = state.machines.lock().await;
            machines.insert(
                machine_id.clone(),
                Machine {
                    id: machine_id.as_str().to_string(),
                    name: "machine-a".into(),
                    node_id: node_id.clone(),
                    state: MachineState::Running,
                    command: "printf 'ok'".into(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                },
            );
        }

        let response = worker_report_handler(
            State(state.clone()),
            ExtractJson(MachineReportRequest {
                node_id: node_id.clone(),
                machine_id: machine_id.clone(),
                state: MachineState::Succeeded,
                exit_code: Some(0),
                stdout: "ok".into(),
                stderr: String::new(),
            }),
        )
        .await
        .unwrap();

        assert_eq!(
            response.message,
            format!("Recorded machine '{}' result as Succeeded", machine_id)
        );

        let machines = state.machines.lock().await;
        let machine = machines.get(&machine_id).unwrap();
        assert_eq!(machine.state, MachineState::Succeeded);
        assert_eq!(machine.exit_code, Some(0));
        assert_eq!(machine.stdout, "ok");
    }

    #[tokio::test]
    async fn worker_report_completes_drain_when_last_active_machine_finishes() {
        let state = AppState {
            app_port: 3000,
            machines: Default::default(),
            nodes: Default::default(),
            launched_workers: Default::default(),
            launcher: Arc::new(NoopWorkerLauncher),
        };
        let node_id = NodeId::new("n1");
        let machine_id = MachineId::new();

        {
            let mut nodes = state.nodes.lock().await;
            nodes.insert(
                node_id.clone(),
                Node {
                    id: node_id.clone(),
                    name: "n1".into(),
                    observed_state: NodeState::Running,
                    desired_state: NodeState::Running,
                    supports_machine_execution: true,
                    cordoned: true,
                    draining: true,
                    last_heartbeat: tokio::time::Instant::now(),
                },
            );
        }

        {
            let mut machines = state.machines.lock().await;
            machines.insert(
                machine_id.clone(),
                Machine {
                    id: machine_id.as_str().to_string(),
                    name: "machine-a".into(),
                    node_id: node_id.clone(),
                    state: MachineState::Running,
                    command: "printf 'ok'".into(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                },
            );
        }

        let _ = worker_report_handler(
            State(state.clone()),
            ExtractJson(MachineReportRequest {
                node_id: node_id.clone(),
                machine_id,
                state: MachineState::Succeeded,
                exit_code: Some(0),
                stdout: "ok".into(),
                stderr: String::new(),
            }),
        )
        .await
        .unwrap();

        let nodes = state.nodes.lock().await;
        let node = nodes.get(&node_id).unwrap();
        assert!(node.cordoned);
        assert!(!node.draining);
    }
}
