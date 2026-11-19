//! A project groups services; dependencies order how they start.

use crate::error::{AppError, AppResult, ErrorKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// Empty on create; the repository assigns a UUID.
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub root_dir: String,
    #[serde(default)]
    pub archived: bool,
}

impl Project {
    pub fn validate(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::new(
                ErrorKind::Validation,
                "A project needs a name.",
            ));
        }
        Ok(())
    }
}

/// What starting a project does when this dependency fails to come up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnFailure {
    /// Stop starting anything that depends on it (the default).
    Abort,
    /// Start the dependants anyway.
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dep {
    pub depends_on_id: String,
    pub on_failure: OnFailure,
}

/// One stored edge: `service_id` needs `depends_on_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub service_id: String,
    pub depends_on_id: String,
    pub on_failure: OnFailure,
}
