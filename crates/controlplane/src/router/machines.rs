use crate::router::state::{AppState, NodeStore};
use axum::extract::{Query, State};
use axum::{
    Router,
    routing::{get, post},
};
use common::{Machine, MachineId, Node};
use rand::seq::IteratorRandom;
use serde::Deserialize;
use tracing::info;

#[derive(Debug, Deserialize)]
struct SetParams {
    machine_name: String,
}

#[derive(Debug, Deserialize)]
struct StopParams {
    machine_id: MachineId,
}

#[derive(Debug, Deserialize)]
struct ShowParams {
    machine_id: Option<MachineId>,
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

async fn launch_handler(State(state): State<AppState>, Query(params): Query<SetParams>) -> String {
    let mut machines = state.machines.lock().await;

    for (id, machine) in machines.iter() {
        if machine.name == params.machine_name {
            return format!(
                "Machine name '{}' already exists with id={}",
                params.machine_name,
                id.as_str()
            );
        }
    }

    let node = match pick_random_node(&state.nodes).await {
        Some(node) => node,
        None => {
            return format!(
                "No schedulable nodes available to assign machine '{}'. Please add or uncordon a running node before launching machines.",
                params.machine_name
            );
        }
    };

    info!(
        "Assigned machine '{}' to node '{}'",
        params.machine_name, node.name
    );

    let machine_id = MachineId::new();
    let machine = Machine {
        node_id: node.id,
        id: machine_id.as_str().to_string(),
        name: params.machine_name.clone(),
        state: common::MachineState::Pending,
    };

    machines.insert(machine_id.clone(), machine);

    format!(
        "Stored machine: id={}, name={}",
        machine_id.as_str(),
        params.machine_name
    )
}

async fn stop_handler(State(state): State<AppState>, Query(params): Query<StopParams>) -> String {
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
                .filter(|m| m.node_id == machine_node_id)
                .count();

            let mut nodes = state.nodes.lock().await;
            if let Some(node) = nodes.get_mut(&machine_node_id) {
                if node.draining && remaining == 0 {
                    node.draining = false;
                }
            }

            format!(
                "Removed machine: id={}, name={}",
                params.machine_id, machine_name
            )
        }
        None => format!("No machine found with id={}", params.machine_id),
    }
}

async fn show_handler(State(state): State<AppState>, Query(params): Query<ShowParams>) -> String {
    let map = state.machines.lock().await;

    if map.is_empty() {
        return "No machines stored".to_string();
    }

    match params.machine_id.as_ref() {
        Some(machine_id_str) => {
            let machine_id = MachineId::from(machine_id_str.clone());

            match map.get(&machine_id) {
                Some(machine) => {
                    format!("Machine found: id={}, name={}", machine_id, machine.name)
                }
                None => format!("No machine found with id={}", machine_id),
            }
        }
        None => {
            let mut lines = Vec::new();
            for (machine_id, machine) in map.iter() {
                lines.push(format!("id={}, name={}", machine_id, machine.name));
            }
            lines.join("\n")
        }
    }
}
pub fn machines_router() -> Router<AppState> {
    Router::new()
        .route("/machines/launch", post(launch_handler))
        .route("/machines/stop", post(stop_handler))
        .route("/machines/show", get(show_handler))
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
            }),
        )
        .await;

        assert_eq!(
            response,
            "No schedulable nodes available to assign machine 'machine-a'. Please add or uncordon a running node before launching machines."
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
            response,
            format!("Removed machine: id={}, name={}", machine_id, "machine-a")
        );

        let nodes = state.nodes.lock().await;
        let node = nodes.get(&node_id).unwrap();
        assert!(node.cordoned);
        assert!(!node.draining);
    }
}
