mod background;
mod launcher;
mod router;
mod settings;

use crate::background::spawn_reaper;
use crate::launcher::LocalProcessLauncher;
use crate::router::AppState;
use settings::Settings;
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let settings: Settings = Settings::new();
    let launcher = Arc::new(LocalProcessLauncher::from_path_or_default(
        settings.worker_binary_path.clone(),
    )?);
    let state = AppState {
        app_port: settings.app_port,
        machines: Default::default(),
        nodes: Default::default(),
        launched_workers: Default::default(),
        launcher,
    };
    let app = router::app_router(state.clone());
    let _reaper = spawn_reaper(state.clone());

    info!("Starting MTC Controlplane on port 3000");
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", settings.app_port))
        .await
        .unwrap();

    axum::serve(listener, app).await?;

    Ok(())
}
