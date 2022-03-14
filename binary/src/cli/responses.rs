use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::to_string;
use anyhow::{*, Result as AnyHow};

#[derive(Serialize, Deserialize)]
pub struct CreateProj {
    user: String,
    action: String,
    new_id: String,
}

impl CreateProj {
    pub fn extract(self) -> AnyHow<String> {
        match (self.user.as_str(), self.action.as_str()) {
            ("login_ok", "ok") if self.new_id != "none" => Ok(self.new_id),
            _ => to_string(&self)
                    .map_err(|e| anyhow!("Unexpected serialization error: {}", e)),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ModdingStatus {
    pub status: String,
}

#[derive(Serialize, Deserialize)]
pub struct GenericResponse {
    user: String,
    action: String,
}

impl GenericResponse {
    pub fn ok(self) -> AnyHow<()> {
        match self.action.as_str() {
            "ok" => Ok(()),
            _ => to_string(&self)
                    .map_err(|e| anyhow!("Unexpected serialization error: {}", e))
                    .map(|_| ())
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Checksum {
    pub sum: u32,
}

#[derive(Serialize, Deserialize)]
pub struct ProjectList {
    user: String,
    projects: HashMap<String, String>,
}

impl ProjectList {
    pub fn extract(self) -> AnyHow<String> {
        match self.user.as_str() {
            "login_ok" => {
                to_string(&self.projects)
                    .map_err(|e| anyhow!("Unexpected serialization error: {}", e))
            }
            _ => {
                to_string(&self)
                    .map_err(|e| anyhow!("Unexpected serialization error: {}", e))
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum IDChanges {
    Failure(String),
    Success(HashMap<String, String>),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ModSubmission {
    user: String,
    id_changes: IDChanges,
}

impl ModSubmission {
    pub fn ok(self) -> Result<HashMap<String, String>> {
        match (self.user.as_str(), &self.id_changes) {
            ("login_ok", IDChanges::Success(new_ids)) => Ok(new_ids.clone()),
            _ => to_string(&self)
                    .map(|_| HashMap::new())
                    .map_err(|e| anyhow!("Unexpected serialization error: {}", e))
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Patches {
    pub patched: Vec<String>,
}
