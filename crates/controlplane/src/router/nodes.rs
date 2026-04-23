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
        node.state = NodeState::Running;
        "Heartbeat received"
    } else {
        warn!("Node not found {}", params.node_id.as_str());
        "Node not found"
    }
}

pub fn nodes_router() -> Router<AppState> {
    Router::new()
        .route("/workers/register", post(register_handler))
        .route("/workers/deregister", post(deregister_handler))
        .route("/workers/heartbeat", post(heartbeat_handler))
}
