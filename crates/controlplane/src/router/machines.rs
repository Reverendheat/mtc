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
    nodes.values().choose(&mut rng).cloned()
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
pub fn machines_router() -> Router<AppState> {
    Router::new()
        .route("/machines/launch", post(launch_handler))
        .route("/machines/stop", post(stop_handler))
        .route("/machines/show", get(show_handler))
}
