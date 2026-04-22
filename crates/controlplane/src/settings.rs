use config::{Config, Environment};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub app_port: u16,
    pub control_plane_url: String,
}

impl Settings {
    pub fn new() -> Self {
        Config::builder()
            .add_source(Environment::default().separator("_"))
            .build()
            .expect("failed to read configuration from environment")
            .try_deserialize()
            .expect("missing or invalid environment variables for Settings")
    }
}
