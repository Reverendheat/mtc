mod background;
mod settings;

use crate::background::{spawn_heartbeat, spawn_machine_runner};
use reqwest::Client;
use settings::Settings;
use std::future::pending;
use tracing::info;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let settings: Settings = Settings::new();
    let client = Client::new();
    let node_id = settings.node_id.clone();

    client
        .post(format!("{}/workers/register", settings.control_plane_url))
        .query(&[
            ("node_id", node_id.as_str()),
            ("supports_machine_execution", "true"),
        ])
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let _heartbeat = spawn_heartbeat(
        client.clone(),
        node_id.clone(),
        settings.control_plane_url.clone(),
    );
    let _runner = spawn_machine_runner(
        client.clone(),
        node_id.clone(),
        settings.control_plane_url.clone(),
    );

    info!("Starting worker node '{}'", node_id);
    pending::<()>().await;
}
