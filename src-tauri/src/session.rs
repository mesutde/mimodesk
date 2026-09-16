//! Remote session placeholders for screen/input milestones.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSession {
    pub peer: String,
    pub started_at: u64,
    pub role: SessionRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionRole {
    Host,
    Client,
}
