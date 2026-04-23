mod background;
mod router;
mod settings;

use crate::background::spawn_reaper;
use crate::router::AppState;
use settings::Settings;
use tracing::info;
use tracing_subscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let settings: Settings = Settings::new();
    let state = AppState {
        machines: Default::default(),
        nodes: Default::default(),
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
