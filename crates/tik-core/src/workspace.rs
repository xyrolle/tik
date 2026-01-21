use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::fs;
use crate::schema::SchemaRegistry;
use crate::timeutil;
use crate::{Result, TikError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub schema_version: String,
    pub default_project: String,
}

impl WorkspaceConfig {
    pub fn new(default_project: &str) -> Result<Self> {
        let name = validate_project_name(default_project)?;
        Ok(Self {
            schema_version: "1.0".to_string(),
            default_project: name,
        })
    }

    pub fn load(path: &Path, schema_dir: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)?;
        let workspace: WorkspaceConfig = serde_json::from_str(&raw)
            .map_err(|err| TikError::Config(format!("invalid workspace json: {err}")))?;
        let schemas = SchemaRegistry::load(schema_dir)?;
        schemas.validate_workspace(&workspace)?;
        Ok(workspace)
    }

    pub fn write(path: &Path, workspace: &WorkspaceConfig, schema_dir: &Path) -> Result<()> {
        let schemas = SchemaRegistry::load(schema_dir)?;
        schemas.validate_workspace(workspace)?;
        let raw = serde_json::to_string_pretty(workspace)
            .map_err(|err| TikError::Config(format!("serialize workspace: {err}")))?;
        fs::write_string_atomic(path, &raw)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMeta {
    pub schema_version: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub description: String,
}

impl ProjectMeta {
    pub fn new(name: &str, description: Option<&str>) -> Result<Self> {
        let name = validate_project_name(name)?;
        let now = timeutil::now_rfc3339()?;
        Ok(Self {
            schema_version: "1.0".to_string(),
            name,
            created_at: now.clone(),
            updated_at: now,
            description: description.unwrap_or_default().to_string(),
        })
    }

    pub fn touch(&mut self) -> Result<()> {
        self.updated_at = timeutil::now_rfc3339()?;
        Ok(())
    }

    pub fn load(path: &Path, schema_dir: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)?;
        let project: ProjectMeta = serde_json::from_str(&raw)
            .map_err(|err| TikError::Config(format!("invalid project json: {err}")))?;
        let schemas = SchemaRegistry::load(schema_dir)?;
        schemas.validate_project(&project)?;
        Ok(project)
    }

    pub fn write(path: &Path, project: &ProjectMeta, schema_dir: &Path) -> Result<()> {
        let schemas = SchemaRegistry::load(schema_dir)?;
        schemas.validate_project(project)?;
        let raw = serde_json::to_string_pretty(project)
            .map_err(|err| TikError::Config(format!("serialize project: {err}")))?;
        fs::write_string_atomic(path, &raw)
    }
}

pub fn projects_dir(tik_root: &Path) -> PathBuf {
    tik_root.join("projects")
}

pub fn project_root(tik_root: &Path, name: &str) -> Result<PathBuf> {
    let name = validate_project_name(name)?;
    Ok(projects_dir(tik_root).join(name))
}

pub fn project_meta_path(project_root: &Path) -> PathBuf {
    project_root.join("project.json")
}

pub fn project_config_path(project_root: &Path) -> PathBuf {
    project_root.join("config.json")
}

pub fn workspace_path(tik_root: &Path) -> PathBuf {
    tik_root.join("workspace.json")
}

pub fn validate_project_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(TikError::Config("project name is required".to_string()));
    }
    if name.len() > 64 {
        return Err(TikError::Config("project name too long".to_string()));
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Err(TikError::Config(
            "project name must be alphanumeric, '_' or '-'".to_string(),
        ));
    }
    if !name
        .chars()
        .next()
        .map(|ch| ch.is_ascii_alphanumeric())
        .unwrap_or(false)
    {
        return Err(TikError::Config(
            "project name must start with alphanumeric".to_string(),
        ));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn project_name_validation() {
        assert!(validate_project_name("alpha").is_ok());
        assert!(validate_project_name("alpha-1").is_ok());
        assert!(validate_project_name("alpha_1").is_ok());
        assert!(validate_project_name("-bad").is_err());
        assert!(validate_project_name("bad space").is_err());
    }

    #[test]
    fn workspace_round_trip() {
        let dir = tempdir().unwrap();
        let schema_dir = dir.path().join("schema");
        fs::ensure_dir(&schema_dir).unwrap();
        crate::schema::write_default_schemas(&schema_dir).unwrap();
        let workspace = WorkspaceConfig::new("default").unwrap();
        let path = dir.path().join("workspace.json");
        WorkspaceConfig::write(&path, &workspace, &schema_dir).unwrap();
        let loaded = WorkspaceConfig::load(&path, &schema_dir).unwrap();
        assert_eq!(loaded.default_project, "default");
    }

    #[test]
    fn project_round_trip() {
        let dir = tempdir().unwrap();
        let schema_dir = dir.path().join("schema");
        fs::ensure_dir(&schema_dir).unwrap();
        crate::schema::write_default_schemas(&schema_dir).unwrap();
        let project = ProjectMeta::new("alpha", Some("Project alpha")).unwrap();
        let path = dir.path().join("project.json");
        ProjectMeta::write(&path, &project, &schema_dir).unwrap();
        let loaded = ProjectMeta::load(&path, &schema_dir).unwrap();
        assert_eq!(loaded.name, "alpha");
        assert_eq!(loaded.description, "Project alpha");
    }
}
