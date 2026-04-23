use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use common::NodeId;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct WorkerLaunchSpec {
    pub node_id: NodeId,
    pub app_port: u16,
    pub control_plane_url: String,
}

#[allow(dead_code)]
pub struct LaunchedWorker {
    pub node_id: NodeId,
    pub app_port: u16,
    pub launched_at: SystemTime,
    pub process_id: Option<u32>,
    pub backend: &'static str,
    pub child: Child,
}

pub type WorkerLaunchStore = Arc<Mutex<HashMap<NodeId, LaunchedWorker>>>;

#[async_trait]
pub trait WorkerLauncher: Send + Sync {
    async fn launch(&self, spec: WorkerLaunchSpec) -> Result<LaunchedWorker>;
    fn backend_name(&self) -> &'static str;
}

pub type SharedWorkerLauncher = Arc<dyn WorkerLauncher>;

pub struct LocalProcessLauncher {
    worker_binary_path: PathBuf,
}

impl LocalProcessLauncher {
    pub fn new(worker_binary_path: PathBuf) -> Self {
        Self { worker_binary_path }
    }

    pub fn from_path_or_default(worker_binary_path: Option<PathBuf>) -> Result<Self> {
        Ok(Self::new(
            worker_binary_path.unwrap_or(default_worker_binary_path()?),
        ))
    }
}

#[async_trait]
impl WorkerLauncher for LocalProcessLauncher {
    async fn launch(&self, spec: WorkerLaunchSpec) -> Result<LaunchedWorker> {
        let child = Command::new(&self.worker_binary_path)
            .env("NODE_ID", spec.node_id.as_str())
            .env("APP_PORT", spec.app_port.to_string())
            .env("CONTROL_PLANE_URL", &spec.control_plane_url)
            .spawn()
            .with_context(|| {
                format!(
                    "failed to launch worker binary at '{}'",
                    self.worker_binary_path.display()
                )
            })?;

        let process_id = child.id();

        Ok(LaunchedWorker {
            node_id: spec.node_id,
            app_port: spec.app_port,
            launched_at: SystemTime::now(),
            process_id,
            backend: self.backend_name(),
            child,
        })
    }

    fn backend_name(&self) -> &'static str {
        "local-process"
    }
}

fn default_worker_binary_path() -> Result<PathBuf> {
    let current_exe = std::env::current_exe().context("failed to determine current executable")?;
    let parent = current_exe
        .parent()
        .ok_or_else(|| anyhow!("control plane executable has no parent directory"))?;
    Ok(parent.join(worker_binary_name()))
}

fn worker_binary_name() -> &'static str {
    if cfg!(windows) {
        "mtcworker.exe"
    } else {
        "mtcworker"
    }
}

pub async fn allocate_worker_port() -> Result<u16> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to allocate a local port for worker")?;
    let port = listener
        .local_addr()
        .context("failed to inspect allocated worker port")?
        .port();
    drop(listener);
    Ok(port)
}

#[cfg(test)]
pub struct NoopWorkerLauncher;

#[cfg(test)]
#[async_trait]
impl WorkerLauncher for NoopWorkerLauncher {
    async fn launch(&self, spec: WorkerLaunchSpec) -> Result<LaunchedWorker> {
        let child = if cfg!(windows) {
            let mut command = Command::new("cmd");
            command.args(["/C", "exit", "0"]);
            command.spawn()?
        } else {
            let mut command = Command::new("sh");
            command.args(["-c", "exit 0"]);
            command.spawn()?
        };

        Ok(LaunchedWorker {
            node_id: spec.node_id,
            app_port: spec.app_port,
            launched_at: SystemTime::now(),
            process_id: child.id(),
            backend: "noop",
            child,
        })
    }

    fn backend_name(&self) -> &'static str {
        "noop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_points_at_worker_binary_name() {
        let path = default_worker_binary_path().unwrap();
        assert!(path.ends_with(worker_binary_name()));
    }
}
