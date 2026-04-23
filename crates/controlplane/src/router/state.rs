use common::{Machine, MachineId, Node, NodeId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type MachineStore = Arc<Mutex<HashMap<MachineId, Machine>>>;
pub type NodeStore = Arc<Mutex<HashMap<NodeId, Node>>>;

#[derive(Clone)]
pub struct AppState {
    pub machines: MachineStore,
    pub nodes: NodeStore,
}
