use serde::{Deserialize, Serialize};
use std::fmt::Display;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct NodeId(String);

impl NodeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct MachineId(String);

impl MachineId {
    pub fn new() -> Self {
        MachineId(Uuid::new_v4().to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for MachineId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for MachineId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MachineState {
    Pending,
    Running,
    Succeeded,
    Stopped,
    Failed,
}

impl MachineState {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Machine {
    pub id: String,
    pub name: String,
    pub node_id: NodeId,
    pub state: MachineState,
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Display for Machine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Machine {{ id: {}, name: {}, node_id: {}, state: {:?} }}",
            self.id, self.name, self.node_id.0, self.state
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeState {
    Pending,
    Running,
    Stale,
    Timeout,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    pub observed_state: NodeState,
    pub desired_state: NodeState,
    pub supports_machine_execution: bool,
    pub cordoned: bool,
    pub draining: bool,
    pub last_heartbeat: tokio::time::Instant,
}

impl Node {
    pub fn is_schedulable(&self) -> bool {
        self.observed_state == NodeState::Running
            && self.desired_state == NodeState::Running
            && self.supports_machine_execution
            && !self.cordoned
            && !self.draining
    }
}
