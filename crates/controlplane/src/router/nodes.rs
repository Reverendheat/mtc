use crate::router::state::AppState;
use axum::extract::{Query, State};
use axum::{Router, routing::post};
use common::{Node, NodeId, NodeState};
use serde::Deserialize;
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
struct HeartbeatParams {
    node_id: NodeId,
}

#[derive(Debug, Deserialize)]
struct NodeActionParams {
    node_id: NodeId,
}

async fn register_handler(
    state: State<AppState>,
    Query(params): Query<HeartbeatParams>,
) -> &'static str {
    info!("Registering worker '{}'", params.node_id.as_str());
    let mut nodes = state.nodes.lock().await;

    if nodes.contains_key(&params.node_id) {
        "Node already registered"
    } else {
        let node = Node {
            id: params.node_id.clone(),
            name: format!("Node-{}", params.node_id.as_str()),
            state: NodeState::Running,
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
        if matches!(node.state, NodeState::Stale | NodeState::Timeout | NodeState::Pending) {
            node.state = NodeState::Running;
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

async fn drain_handler(
    state: State<AppState>,
    Query(params): Query<NodeActionParams>,
) -> String {
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

    format!(
        "Node draining; {} machine(s) still assigned",
        machine_count
    )
}

pub fn nodes_router() -> Router<AppState> {
    Router::new()
        .route("/workers/register", post(register_handler))
        .route("/workers/deregister", post(deregister_handler))
        .route("/workers/heartbeat", post(heartbeat_handler))
        .route("/workers/cordon", post(cordon_handler))
        .route("/workers/uncordon", post(uncordon_handler))
        .route("/workers/drain", post(drain_handler))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::state::AppState;

    #[tokio::test]
    async fn heartbeats_do_not_clear_cordon_or_drain_flags() {
        let state = AppState {
            machines: Default::default(),
            nodes: Default::default(),
        };
        let node_id = NodeId::new("n1");

        {
            let mut nodes = state.nodes.lock().await;
            nodes.insert(
                node_id.clone(),
                Node {
                    id: node_id.clone(),
                    name: "n1".into(),
                    state: NodeState::Stale,
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
        assert_eq!(node.state, NodeState::Running);
        assert!(node.cordoned);
        assert!(node.draining);
    }

    #[tokio::test]
    async fn drain_marks_node_unschedulable() {
        let state = AppState {
            machines: Default::default(),
            nodes: Default::default(),
        };
        let node_id = NodeId::new("n1");

        {
            let mut nodes = state.nodes.lock().await;
            nodes.insert(
                node_id.clone(),
                Node {
                    id: node_id.clone(),
                    name: "n1".into(),
                    state: NodeState::Running,
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
}
