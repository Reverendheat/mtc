use axum::{Router, routing::get};
use common::NodeId;
use reqwest::Client;

async fn spawn_heartbeat(client: Client, node_id: NodeId) {
    let node_id = node_id.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(5));

        loop {
            ticker.tick().await;
            client
                .post("http://localhost:3000/workers/heartbeat")
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
    let args = std::env::args().collect::<Vec<String>>();
    let port = if args.len() > 1 {
        args[1].parse::<u16>().unwrap_or(3000)
    } else {
        3000
    };

    let client = Client::new();
    let node_id = NodeId::new("worker-node-1");

    client
        .post("http://localhost:3000/workers/register")
        .query(&[("node_id", node_id.as_str())])
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    spawn_heartbeat(client.clone(), node_id).await;

    let app = Router::new().route("/", get(|| async { "Worker Node" }));

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
