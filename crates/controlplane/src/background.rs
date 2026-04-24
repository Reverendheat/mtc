use crate::router::AppState;
use common::NodeState;

pub fn spawn_reaper(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(10));

        loop {
            ticker.tick().await;

            let now = tokio::time::Instant::now();
            let mut nodes = state.nodes.lock().await;

            for node in nodes.values_mut() {
                if now.duration_since(node.last_heartbeat) > tokio::time::Duration::from_secs(10) {
                    node.observed_state = NodeState::Stale;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::NoopWorkerLauncher;
    use common::{Node, NodeId};
    use std::sync::Arc;
    use tokio::time::{self, Duration};

    #[tokio::test(start_paused = true)]
    async fn marks_stale_nodes() {
        let state = AppState {
            app_port: 3000,
            machines: Default::default(),
            nodes: Default::default(),
            launched_workers: Default::default(),
            launcher: Arc::new(NoopWorkerLauncher),
        };

        let node_id = NodeId::new("n1");

        {
            let mut nodes = state.nodes.lock().await;
            nodes.insert(
                node_id.clone(),
                Node {
                    id: node_id.clone(),
                    name: "n1".into(),
                    last_heartbeat: tokio::time::Instant::now(),
                    observed_state: NodeState::Running,
                    desired_state: NodeState::Running,
                    supports_machine_execution: true,
                    cordoned: false,
                    draining: false,
                },
            );
        }

        let handle = spawn_reaper(state.clone());

        time::advance(Duration::from_secs(21)).await;
        tokio::task::yield_now().await;

        let nodes = state.nodes.lock().await;
        let node = nodes.get(&node_id).unwrap();
        assert_eq!(node.observed_state, NodeState::Stale);
        assert_eq!(node.desired_state, NodeState::Running);

        handle.abort();
    }
}
