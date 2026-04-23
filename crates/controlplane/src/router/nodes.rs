use crate::router::state::AppState;
use axum::extract::{Query, State};
use axum::{Json, Router, routing::post};
use common::{Node, NodeId, NodeState};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct HeartbeatParams {
    node_id: NodeId,
}

#[derive(Debug, Deserialize)]
struct NodeActionParams {
    node_id: NodeId,
}

#[derive(Debug, Deserialize)]
struct LaunchWorkerParams {
    node_id: Option<NodeId>,
    app_port: Option<u16>,
}

#[derive(Debug, Serialize)]
struct LaunchWorkerResponse {
    node_id: NodeId,
    app_port: u16,
    backend: &'static str,
    process_id: Option<u32>,
    observed_state: NodeState,
    desired_state: NodeState,
}

async fn register_handler(
    state: State<AppState>,
    Query(params): Query<HeartbeatParams>,
) -> &'static str {
    info!("Registering worker '{}'", params.node_id.as_str());
    let mut nodes = state.nodes.lock().await;

    if let Some(node) = nodes.get_mut(&params.node_id) {
        let was_pending = node.observed_state == NodeState::Pending;
        node.last_heartbeat = tokio::time::Instant::now();
        node.observed_state = NodeState::Running;
        node.desired_state = NodeState::Running;

        if was_pending {
            info!("Registered worker '{}'", params.node_id.as_str());
            "Worker registered"
        } else {
            "Node already registered"
        }
    } else {
        let node = Node {
            id: params.node_id.clone(),
            name: format!("Node-{}", params.node_id.as_str()),
            observed_state: NodeState::Running,
            desired_state: NodeState::Running,
            cordoned: false,
            draining: false,
            last_heartbeat: tokio::time::Instant::now(),
        };
        nodes.insert(params.node_id.clone(), node);
        info!("Registered worker '{}'", params.node_id.as_str());
        "Worker registered"
    }
}

async fn deregister_handler() -> &'static str {
    "Worker deregistered"
}

async fn heartbeat_handler(
    state: State<AppState>,
    Query(params): Query<HeartbeatParams>,
) -> &'static str {
    info!("Received heartbeat from node '{}'", params.node_id.as_str());
    let mut nodes = state.nodes.lock().await;

    if let Some(node) = nodes.get_mut(&params.node_id) {
        node.last_heartbeat = tokio::time::Instant::now();
        if matches!(
            node.observed_state,
            NodeState::Stale | NodeState::Timeout | NodeState::Pending
        ) {
            node.observed_state = NodeState::Running;
        }
        "Heartbeat received"
    } else {
        warn!("Node not found {}", params.node_id.as_str());
        "Node not found"
    }
}

async fn cordon_handler(
    state: State<AppState>,
    Query(params): Query<NodeActionParams>,
) -> &'static str {
    let mut nodes = state.nodes.lock().await;

    if let Some(node) = nodes.get_mut(&params.node_id) {
        node.cordoned = true;
        "Node cordoned"
    } else {
        warn!("Node not found {}", params.node_id.as_str());
        "Node not found"
    }
}

async fn uncordon_handler(
    state: State<AppState>,
    Query(params): Query<NodeActionParams>,
) -> &'static str {
    let mut nodes = state.nodes.lock().await;

    if let Some(node) = nodes.get_mut(&params.node_id) {
        node.cordoned = false;
        node.draining = false;
        "Node uncordoned"
    } else {
        warn!("Node not found {}", params.node_id.as_str());
        "Node not found"
    }
}

async fn drain_handler(state: State<AppState>, Query(params): Query<NodeActionParams>) -> String {
    let mut nodes = state.nodes.lock().await;

    let Some(node) = nodes.get_mut(&params.node_id) else {
        warn!("Node not found {}", params.node_id.as_str());
        return "Node not found".to_string();
    };

    node.cordoned = true;
    node.draining = true;
    drop(nodes);

    let machines = state.machines.lock().await;
    let machine_count = machines
        .values()
        .filter(|machine| machine.node_id == params.node_id)
        .count();

    format!("Node draining; {} machine(s) still assigned", machine_count)
}

async fn launch_handler(
    State(state): State<AppState>,
    Query(params): Query<LaunchWorkerParams>,
) -> Result<Json<LaunchWorkerResponse>, (axum::http::StatusCode, String)> {
    let node_id = params
        .node_id
        .unwrap_or_else(|| NodeId::new(format!("worker-node-{}", short_id())));
    let app_port = match params.app_port {
        Some(app_port) => app_port,
        None => crate::launcher::allocate_worker_port()
            .await
            .map_err(internal_error)?,
    };
    let placeholder = Node {
        id: node_id.clone(),
        name: format!("Node-{}", node_id.as_str()),
        observed_state: NodeState::Pending,
        desired_state: NodeState::Running,
        cordoned: false,
        draining: false,
        last_heartbeat: tokio::time::Instant::now(),
    };

    {
        let mut nodes = state.nodes.lock().await;
        if nodes.contains_key(&node_id) {
            return Err((
                axum::http::StatusCode::CONFLICT,
                format!("Node '{}' already exists", node_id),
            ));
        }
        nodes.insert(node_id.clone(), placeholder);
    }

    let launch_result = state
        .launcher
        .launch(crate::launcher::WorkerLaunchSpec {
            node_id: node_id.clone(),
            app_port,
            control_plane_url: format!("http://127.0.0.1:{}", state.app_port),
        })
        .await;

    match launch_result {
        Ok(launched_worker) => {
            let process_id = launched_worker.process_id;
            let backend = launched_worker.backend;
            state
                .launched_workers
                .lock()
                .await
                .insert(node_id.clone(), launched_worker);

            Ok(Json(LaunchWorkerResponse {
                node_id,
                app_port,
                backend,
                process_id,
                observed_state: NodeState::Pending,
                desired_state: NodeState::Running,
            }))
        }
        Err(error) => {
            state.nodes.lock().await.remove(&node_id);
            Err(internal_error(error))
        }
    }
}

pub fn nodes_router() -> Router<AppState> {
    Router::new()
        .route("/workers/launch", post(launch_handler))
        .route("/workers/register", post(register_handler))
        .route("/workers/deregister", post(deregister_handler))
        .route("/workers/heartbeat", post(heartbeat_handler))
        .route("/workers/cordon", post(cordon_handler))
        .route("/workers/uncordon", post(uncordon_handler))
        .route("/workers/drain", post(drain_handler))
}

fn short_id() -> String {
    Uuid::new_v4().simple().to_string()[..8].to_string()
}

fn internal_error(error: impl std::fmt::Display) -> (axum::http::StatusCode, String) {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        error.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::{LaunchedWorker, SharedWorkerLauncher, WorkerLaunchSpec, WorkerLauncher};
    use crate::router::state::AppState;
    use async_trait::async_trait;
    use std::sync::Arc;
    use std::time::SystemTime;
    use tokio::process::Command;
    use tokio::sync::Mutex;

    fn test_state(launcher: SharedWorkerLauncher) -> AppState {
        AppState {
            app_port: 3000,
            machines: Default::default(),
            nodes: Default::default(),
            launched_workers: Default::default(),
            launcher,
        }
    }

    struct FakeLauncher {
        launches: Arc<Mutex<Vec<WorkerLaunchSpec>>>,
    }

    #[async_trait]
    impl WorkerLauncher for FakeLauncher {
        async fn launch(&self, spec: WorkerLaunchSpec) -> anyhow::Result<LaunchedWorker> {
            self.launches.lock().await.push(spec.clone());

            let child = if cfg!(windows) {
                let mut command = Command::new("cmd");
                command.args(["/C", "exit", "0"]);
                command.spawn()?
            } else {
                let mut command = Command::new("sh");
                command.args(["-c", "exit 0"]);
                command.spawn()?
            };

            Ok(LaunchedWorker {
                node_id: spec.node_id,
                app_port: spec.app_port,
                launched_at: SystemTime::now(),
                process_id: child.id(),
                backend: "fake",
                child,
            })
        }

        fn backend_name(&self) -> &'static str {
            "fake"
        }
    }

    #[tokio::test]
    async fn heartbeats_do_not_clear_cordon_or_drain_flags() {
        let state = test_state(Arc::new(FakeLauncher {
            launches: Arc::new(Mutex::new(Vec::new())),
        }));
        let node_id = NodeId::new("n1");

        {
            let mut nodes = state.nodes.lock().await;
            nodes.insert(
                node_id.clone(),
                Node {
                    id: node_id.clone(),
                    name: "n1".into(),
                    observed_state: NodeState::Stale,
                    desired_state: NodeState::Running,
                    cordoned: true,
                    draining: true,
                    last_heartbeat: tokio::time::Instant::now(),
                },
            );
        }

        let response = heartbeat_handler(
            State(state.clone()),
            Query(HeartbeatParams {
                node_id: node_id.clone(),
            }),
        )
        .await;

        assert_eq!(response, "Heartbeat received");

        let nodes = state.nodes.lock().await;
        let node = nodes.get(&node_id).unwrap();
        assert_eq!(node.observed_state, NodeState::Running);
        assert_eq!(node.desired_state, NodeState::Running);
        assert!(node.cordoned);
        assert!(node.draining);
    }

    #[tokio::test]
    async fn drain_marks_node_unschedulable() {
        let state = test_state(Arc::new(FakeLauncher {
            launches: Arc::new(Mutex::new(Vec::new())),
        }));
        let node_id = NodeId::new("n1");

        {
            let mut nodes = state.nodes.lock().await;
            nodes.insert(
                node_id.clone(),
                Node {
                    id: node_id.clone(),
                    name: "n1".into(),
                    observed_state: NodeState::Running,
                    desired_state: NodeState::Running,
                    cordoned: false,
                    draining: false,
                    last_heartbeat: tokio::time::Instant::now(),
                },
            );
        }

        let response = drain_handler(
            State(state.clone()),
            Query(NodeActionParams {
                node_id: node_id.clone(),
            }),
        )
        .await;

        assert_eq!(response, "Node draining; 0 machine(s) still assigned");

        let nodes = state.nodes.lock().await;
        let node = nodes.get(&node_id).unwrap();
        assert!(node.cordoned);
        assert!(node.draining);
        assert!(!node.is_schedulable());
    }

    #[tokio::test]
    async fn launch_creates_placeholder_and_tracks_process() {
        let launches = Arc::new(Mutex::new(Vec::new()));
        let state = test_state(Arc::new(FakeLauncher {
            launches: launches.clone(),
        }));
        let node_id = NodeId::new("launch-me");

        let response = launch_handler(
            State(state.clone()),
            Query(LaunchWorkerParams {
                node_id: Some(node_id.clone()),
                app_port: Some(4010),
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(response.node_id, node_id);
        assert_eq!(response.backend, "fake");
        assert_eq!(response.observed_state, NodeState::Pending);
        assert_eq!(response.desired_state, NodeState::Running);

        let nodes = state.nodes.lock().await;
        let node = nodes.get(&node_id).unwrap();
        assert_eq!(node.observed_state, NodeState::Pending);
        assert_eq!(node.desired_state, NodeState::Running);
        drop(nodes);

        let launched_workers = state.launched_workers.lock().await;
        assert!(launched_workers.contains_key(&node_id));
        drop(launched_workers);

        let launch_specs = launches.lock().await;
        assert_eq!(launch_specs.len(), 1);
        assert_eq!(launch_specs[0].node_id, node_id);
    }
}
