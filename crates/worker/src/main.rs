mod settings;

use axum::{Router, routing::get};
use common::NodeId;
use reqwest::Client;
use settings::Settings;

async fn spawn_heartbeat(client: Client, node_id: NodeId) {
    let node_id = node_id.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(5));

        loop {
            ticker.tick().await;
            client
                .post("http://controlplane:3000/workers/heartbeat")
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
    // get the port from the command line arguments
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

    spawn_heartbeat(client.clone(), node_id.clone()).await;

    let app = Router::new().route(
        "/",
        get(|| async move { format!("Worker Node {}", node_id) }),
    );

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", settings.app_port))
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
