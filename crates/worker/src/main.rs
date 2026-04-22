mod settings;

use axum::{Router, routing::get};
use common::NodeId;
use reqwest::Client;
use settings::Settings;
use tracing::{debug, info};
use tracing_subscriber;

async fn spawn_heartbeat(client: Client, node_id: NodeId, control_plane: String) {
    let node_id = node_id.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(5));

        loop {
            ticker.tick().await;
            debug!("Sending heartbeat for node '{}'", node_id.as_str());
            client
                .post(format!("{}/workers/heartbeat", control_plane))
                .query(&[("node_id", node_id.as_str())])
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap();
        }
    });
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let settings: Settings = Settings::new();

    let client = Client::new();
    let node_id = NodeId::new("worker-node-1");

    client
        .post(format!("{}/workers/register", settings.control_plane_url))
        .query(&[("node_id", node_id.as_str())])
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    spawn_heartbeat(client.clone(), node_id.clone(), settings.control_plane_url).await;

    let app = Router::new().route(
        "/",
        get({
            let node_id = node_id.clone();
            move || async move { format!("Worker Node {}", node_id) }
        }),
    );

    info!(
        "Starting worker node '{}' on port {}",
        node_id, settings.app_port
    );

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", settings.app_port))
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
