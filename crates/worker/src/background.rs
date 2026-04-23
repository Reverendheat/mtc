use common::NodeId;
use reqwest::Client;
use tokio::task::JoinHandle;
use tracing::{debug, error};

pub fn spawn_heartbeat(client: Client, node_id: NodeId, control_plane: String) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(5));
        let url = format!("{}/workers/heartbeat", control_plane.trim_end_matches('/'));

        loop {
            ticker.tick().await;
            debug!("Sending heartbeat for node '{}'", node_id.as_str());

            match client
                .post(&url)
                .query(&[("node_id", node_id.as_str())])
                .send()
                .await
            {
                Ok(resp) => {
                    if let Err(err) = resp.error_for_status() {
                        error!("Heartbeat got non-success status: {err}");
                    }
                }
                Err(err) => {
                    error!("Heartbeat request failed: {err}");
                }
            }
        }
    })
}
