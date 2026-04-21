use axum::{
    Router,
    extract::{Query, State},
    routing::{get, post},
};
use common::{Machine, MachineId, Node, NodeId, NodeState};
use rand::seq::IteratorRandom;
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use tracing::{info, warn};
use tracing_subscriber;

type MachineStore = Arc<Mutex<HashMap<MachineId, Machine>>>;
type NodeStore = Arc<Mutex<HashMap<NodeId, Node>>>;

#[derive(Clone)]
struct AppState {
    machines: MachineStore,
    nodes: NodeStore,
}

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

#[derive(Debug, Deserialize)]
struct HeartbeatParams {
    node_id: NodeId,
}

async fn pick_random_node(state: &NodeStore) -> Option<Node> {
    let nodes = state.lock().await;
    let mut rng = rand::thread_rng();
    nodes.values().choose(&mut rng).cloned()
}

async fn root_handler() -> &'static str {
    "MTC Control plane is running. Use /launch to add machines, /stop to remove machines, and /show to list a machine."
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
                "No available nodes to assign machine '{}'. Please add nodes before launching machines.",
                params.machine_name
            );
        }
    };

    println!(
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
        Some(machine_name) => format!(
            "Removed machine: id={}, name={}",
            params.machine_id, machine_name
        ),
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

async fn register_handler(
    state: State<AppState>,
    Query(params): Query<HeartbeatParams>,
) -> &'static str {
    let mut nodes = state.nodes.lock().await;

    if nodes.contains_key(&params.node_id) {
        "Node already registered"
    } else {
        let node = Node {
            id: params.node_id.clone(),
            name: format!("Node-{}", params.node_id.as_str()),
            state: NodeState::Running,
            last_heartbeat: std::time::Instant::now(),
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
        node.last_heartbeat = std::time::Instant::now();
        node.state = NodeState::Running;
        "Heartbeat received"
    } else {
        warn!("Node not found {}", params.node_id.as_str());
        "Node not found"
    }
}

fn spawn_reaper(state: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(10));

        loop {
            ticker.tick().await;

            let now = std::time::Instant::now();
            let mut nodes = state.nodes.lock().await;

            for node in nodes.values_mut() {
                if now.duration_since(node.last_heartbeat) > std::time::Duration::from_secs(10) {
                    info!(
                        "Node '{}' is stale. Last heartbeat was at {:?}. Marking as stale.",
                        node.name, node.last_heartbeat
                    );
                    node.state = NodeState::Stale;
                }
            }
        }
    });
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let machine_state: MachineStore = Arc::new(Mutex::new(HashMap::new()));
    let node_state: NodeStore = Arc::new(Mutex::new(HashMap::new()));

    let state = AppState {
        machines: machine_state.clone(),
        nodes: node_state.clone(),
    };
    spawn_reaper(state.clone());

    let app = Router::new()
        .route("/", get(root_handler))
        .route("/machines/launch", post(launch_handler))
        .route("/machines/stop", post(stop_handler))
        .route("/machines/show", get(show_handler))
        .route("/workers/register", post(register_handler))
        .route("/workers/deregister", post(deregister_handler))
        .route("/workers/heartbeat", post(heartbeat_handler))
        .with_state(state);

    info!("Starting MTC Controlplane on port 3000");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
