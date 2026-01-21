use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RepoStatus {
    pub repo_root: String,
    pub tik_root: String,
    pub schema_version: String,
    pub config_version: String,
    pub layout_version: String,
    pub index_present: bool,
    pub ticket_count: u64,
    pub milestone_count: u64,
}
