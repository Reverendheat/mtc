use crate::router::state::AppState;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use common::NodeState;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
struct NodeSummary {
    node_id: String,
    name: String,
    observed_state: NodeState,
    desired_state: NodeState,
    cordoned: bool,
    draining: bool,
    machine_count: usize,
    app_port: Option<u16>,
    process_id: Option<u32>,
    backend: Option<&'static str>,
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../../ui/index.html"))
}

async fn app_js_handler() -> Response {
    (
        [("content-type", "application/javascript; charset=utf-8")],
        include_str!("../../ui/app.js"),
    )
        .into_response()
}

async fn styles_handler() -> Response {
    (
        [("content-type", "text/css; charset=utf-8")],
        include_str!("../../ui/styles.css"),
    )
        .into_response()
}

async fn list_nodes_handler(State(state): State<AppState>) -> Json<Vec<NodeSummary>> {
    let nodes = state.nodes.lock().await;
    let launched_workers = state.launched_workers.lock().await;
    let machines = state.machines.lock().await;

    let machine_counts = machines
        .values()
        .fold(HashMap::new(), |mut counts, machine| {
            *counts
                .entry(machine.node_id.as_str().to_string())
                .or_insert(0usize) += 1;
            counts
        });

    let mut summaries = nodes
        .values()
        .map(|node| {
            let launched = launched_workers.get(&node.id);

            NodeSummary {
                node_id: node.id.as_str().to_string(),
                name: node.name.clone(),
                observed_state: node.observed_state.clone(),
                desired_state: node.desired_state.clone(),
                cordoned: node.cordoned,
                draining: node.draining,
                machine_count: *machine_counts.get(node.id.as_str()).unwrap_or(&0),
                app_port: None,
                process_id: launched.and_then(|worker| worker.process_id),
                backend: launched.map(|worker| worker.backend),
            }
        })
        .collect::<Vec<_>>();

    summaries.sort_by(|left, right| left.name.cmp(&right.name));

    Json(summaries)
}

pub fn ui_router() -> Router<AppState> {
    Router::new()
        .route("/", get(index_handler))
        .route("/app.js", get(app_js_handler))
        .route("/styles.css", get(styles_handler))
        .route("/api/nodes", get(list_nodes_handler))
}
