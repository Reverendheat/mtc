mod machines;
mod nodes;
mod state;
mod ui;

pub use state::AppState;

use axum::Router;

pub fn app_router(state: AppState) -> axum::Router {
    Router::new()
        .merge(ui::ui_router())
        .merge(machines::machines_router())
        .merge(nodes::nodes_router())
        .with_state(state)
}
