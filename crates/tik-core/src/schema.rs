use std::path::Path;

use jsonschema::JSONSchema;
use serde_json::Value;

use crate::domain::event::Event;
use crate::domain::milestone::Milestone;
use crate::domain::ticket::Ticket;
use crate::fs;
use crate::{Result, TikError};

const TICKET_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Ticket",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "schema_version",
    "id",
    "title",
    "status",
    "type",
    "priority",
    "severity",
    "assignees",
    "tags",
    "created_at",
    "updated_at",
    "summary",
    "description",
    "acceptance",
    "relations",
    "artifacts",
    "custom"
  ],
  "properties": {
    "schema_version": {
      "type": "string",
      "pattern": "^\\d+\\.\\d+$"
    },
    "id": {
      "type": "string",
      "pattern": "^T-[0-9A-HJKMNP-TV-Z]{26}$"
    },
    "title": {
      "type": "string",
      "minLength": 1
    },
    "status": {
      "type": "string",
      "enum": ["open", "in_progress", "blocked", "closed", "archived"]
    },
    "type": {
      "type": "string",
      "enum": ["feature", "bug", "chore", "task", "spike"]
    },
    "priority": {
      "type": "string",
      "enum": ["low", "medium", "high", "critical"]
    },
    "severity": {
      "type": "string",
      "enum": ["low", "normal", "high", "critical"]
    },
    "assignees": {
      "type": "array",
      "items": {"type": "string"}
    },
    "milestone_id": {
      "type": ["string", "null"],
      "pattern": "^M-[0-9A-HJKMNP-TV-Z]{26}$"
    },
    "tags": {
      "type": "array",
      "items": {"type": "string"}
    },
    "created_at": {
      "type": "string",
      "format": "date-time"
    },
    "updated_at": {
      "type": "string",
      "format": "date-time"
    },
    "closed_at": {
      "type": ["string", "null"],
      "format": "date-time"
    },
    "summary": {
      "type": "string"
    },
    "description": {
      "type": "string"
    },
    "acceptance": {
      "type": "array",
      "items": {"type": "string"}
    },
    "estimate": {
      "type": ["object", "null"],
      "required": ["value", "unit"],
      "properties": {
        "value": {"type": "number"},
        "unit": {"type": "string"}
      },
      "additionalProperties": false
    },
    "due_at": {
      "type": ["string", "null"],
      "format": "date-time"
    },
    "relations": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["type", "id"],
        "properties": {
          "type": {
            "type": "string",
            "enum": ["blocks", "blocked_by", "depends_on", "duplicate", "parent", "child"]
          },
          "id": {"type": "string"}
        },
        "additionalProperties": false
      }
    },
    "artifacts": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["type", "ref"],
        "properties": {
          "type": {"type": "string", "enum": ["file", "url", "commit"]},
          "ref": {"type": "string"}
        },
        "additionalProperties": false
      }
    },
    "custom": {
      "type": "object"
    }
  }
}
"#;

const MILESTONE_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Milestone",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "schema_version",
    "id",
    "title",
    "status",
    "created_at",
    "updated_at",
    "description",
    "tags"
  ],
  "properties": {
    "schema_version": {
      "type": "string",
      "pattern": "^\\d+\\.\\d+$"
    },
    "id": {
      "type": "string",
      "pattern": "^M-[0-9A-HJKMNP-TV-Z]{26}$"
    },
    "title": {
      "type": "string",
      "minLength": 1
    },
    "status": {
      "type": "string",
      "enum": ["open", "closed", "archived"]
    },
    "created_at": {
      "type": "string",
      "format": "date-time"
    },
    "updated_at": {
      "type": "string",
      "format": "date-time"
    },
    "due_at": {
      "type": ["string", "null"],
      "format": "date-time"
    },
    "description": {
      "type": "string"
    },
    "tags": {
      "type": "array",
      "items": {"type": "string"}
    }
  }
}
"#;

const EVENT_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Event",
  "type": "object",
  "additionalProperties": false,
  "required": ["event_id", "ts", "actor", "type", "data"],
  "properties": {
    "event_id": {
      "type": "string",
      "pattern": "^E-[0-9A-HJKMNP-TV-Z]{26}$"
    },
    "ts": {
      "type": "string",
      "format": "date-time"
    },
    "actor": {
      "type": "string",
      "minLength": 1
    },
    "type": {
      "type": "string",
      "minLength": 1
    },
    "data": {
      "type": "object"
    }
  }
}
"#;

const CONFIG_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Config",
  "type": "object",
  "additionalProperties": false,
  "required": ["schema_version", "output_format", "pager", "timezone"],
  "properties": {
    "schema_version": {
      "type": "string",
      "pattern": "^\\d+\\.\\d+$"
    },
    "output_format": {
      "type": "string",
      "enum": ["table", "compact", "json", "jsonl", "yaml", "md", "csv"]
    },
    "pager": {
      "type": "string",
      "enum": ["auto", "always", "never"]
    },
    "timezone": {
      "type": "string",
      "enum": ["UTC", "local"]
    }
  }
}
"#;

const WORKSPACE_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Workspace",
  "type": "object",
  "additionalProperties": false,
  "required": ["schema_version", "default_project"],
  "properties": {
    "schema_version": {
      "type": "string",
      "pattern": "^\\d+\\.\\d+$"
    },
    "default_project": {
      "type": "string",
      "pattern": "^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$"
    }
  }
}
"#;

const PROJECT_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Project",
  "type": "object",
  "additionalProperties": false,
  "required": ["schema_version", "name", "created_at", "updated_at"],
  "properties": {
    "schema_version": {
      "type": "string",
      "pattern": "^\\d+\\.\\d+$"
    },
    "name": {
      "type": "string",
      "pattern": "^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$"
    },
    "created_at": {
      "type": "string",
      "format": "date-time"
    },
    "updated_at": {
      "type": "string",
      "format": "date-time"
    },
    "description": {
      "type": "string"
    }
  }
}
"#;

pub struct SchemaRegistry {
    ticket: JSONSchema,
    milestone: JSONSchema,
    event: JSONSchema,
    config: JSONSchema,
    workspace: JSONSchema,
    project: JSONSchema,
}

impl SchemaRegistry {
    pub fn load(schema_dir: &Path) -> Result<SchemaRegistry> {
        let ticket = compile_schema(&schema_dir.join("ticket.schema.json"))?;
        let milestone = compile_schema(&schema_dir.join("milestone.schema.json"))?;
        let event = compile_schema(&schema_dir.join("event.schema.json"))?;
        let config = compile_schema(&schema_dir.join("config.schema.json"))?;
        let workspace = compile_schema(&schema_dir.join("workspace.schema.json"))?;
        let project = compile_schema(&schema_dir.join("project.schema.json"))?;

        Ok(SchemaRegistry {
            ticket,
            milestone,
            event,
            config,
            workspace,
            project,
        })
    }

    pub fn validate_ticket(&self, ticket: &Ticket) -> Result<()> {
        let value = serde_json::to_value(ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        validate_value(&self.ticket, &value, "ticket")
    }

    pub fn validate_milestone(&self, milestone: &Milestone) -> Result<()> {
        let value = serde_json::to_value(milestone)
            .map_err(|err| TikError::Schema(format!("serialize milestone: {err}")))?;
        validate_value(&self.milestone, &value, "milestone")
    }

    pub fn validate_event(&self, event: &Event) -> Result<()> {
        let value = serde_json::to_value(event)
            .map_err(|err| TikError::Schema(format!("serialize event: {err}")))?;
        validate_value(&self.event, &value, "event")
    }

    pub fn validate_config(&self, config: &crate::Config) -> Result<()> {
        let value = serde_json::to_value(config)
            .map_err(|err| TikError::Schema(format!("serialize config: {err}")))?;
        validate_value(&self.config, &value, "config")
    }

    pub fn validate_workspace(&self, workspace: &crate::workspace::WorkspaceConfig) -> Result<()> {
        let value = serde_json::to_value(workspace)
            .map_err(|err| TikError::Schema(format!("serialize workspace: {err}")))?;
        validate_value(&self.workspace, &value, "workspace")
    }

    pub fn validate_project(&self, project: &crate::workspace::ProjectMeta) -> Result<()> {
        let value = serde_json::to_value(project)
            .map_err(|err| TikError::Schema(format!("serialize project: {err}")))?;
        validate_value(&self.project, &value, "project")
    }
}

fn compile_schema(path: &Path) -> Result<JSONSchema> {
    let raw = fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&raw)
        .map_err(|err| TikError::Schema(format!("invalid schema json: {err}")))?;
    JSONSchema::compile(&json).map_err(|err| TikError::Schema(format!("compile schema: {err}")))
}

fn validate_value(schema: &JSONSchema, value: &Value, label: &str) -> Result<()> {
    if let Err(errors) = schema.validate(value) {
        let mut messages = Vec::new();
        for error in errors {
            messages.push(error.to_string());
        }
        return Err(TikError::Schema(format!(
            "{label} schema invalid: {}",
            messages.join("; ")
        )));
    }
    Ok(())
}

pub fn write_default_schemas(schema_dir: &Path) -> Result<()> {
    fs::ensure_dir(schema_dir)?;

    fs::write_string_atomic(&schema_dir.join("ticket.schema.json"), TICKET_SCHEMA)?;
    fs::write_string_atomic(&schema_dir.join("milestone.schema.json"), MILESTONE_SCHEMA)?;
    fs::write_string_atomic(&schema_dir.join("event.schema.json"), EVENT_SCHEMA)?;
    fs::write_string_atomic(&schema_dir.join("config.schema.json"), CONFIG_SCHEMA)?;
    fs::write_string_atomic(&schema_dir.join("workspace.schema.json"), WORKSPACE_SCHEMA)?;
    fs::write_string_atomic(&schema_dir.join("project.schema.json"), PROJECT_SCHEMA)?;

    Ok(())
}

pub fn ensure_default_schemas(schema_dir: &Path) -> Result<()> {
    fs::ensure_dir(schema_dir)?;

    write_if_missing(&schema_dir.join("ticket.schema.json"), TICKET_SCHEMA)?;
    write_if_missing(&schema_dir.join("milestone.schema.json"), MILESTONE_SCHEMA)?;
    write_if_missing(&schema_dir.join("event.schema.json"), EVENT_SCHEMA)?;
    write_if_missing(&schema_dir.join("config.schema.json"), CONFIG_SCHEMA)?;
    write_if_missing(&schema_dir.join("workspace.schema.json"), WORKSPACE_SCHEMA)?;
    write_if_missing(&schema_dir.join("project.schema.json"), PROJECT_SCHEMA)?;
    Ok(())
}

fn write_if_missing(path: &Path, contents: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    fs::write_string_atomic(path, contents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::Event;
    use crate::domain::milestone::{Milestone, NewMilestone};
    use crate::domain::ticket::{NewTicket, Ticket};
    use crate::workspace::{ProjectMeta, WorkspaceConfig};
    use crate::Config;
    use tempfile::tempdir;

    #[test]
    fn write_and_load_schemas() {
        let dir = tempdir().unwrap();
        write_default_schemas(dir.path()).unwrap();
        let registry = SchemaRegistry::load(dir.path()).unwrap();

        let ticket = Ticket::new(
            NewTicket {
                title: "Test".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );

        let event = Event::note("human", "2026-01-01T00:00:00Z", "note");
        let milestone = Milestone::new(
            NewMilestone {
                title: "Phase 1".to_string(),
                description: None,
                due_at: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        let config = Config::default();
        let workspace = WorkspaceConfig::new("default").unwrap();
        let project = ProjectMeta::new("default", None).unwrap();

        registry.validate_ticket(&ticket).unwrap();
        registry.validate_milestone(&milestone).unwrap();
        registry.validate_event(&event).unwrap();
        registry.validate_config(&config).unwrap();
        registry.validate_workspace(&workspace).unwrap();
        registry.validate_project(&project).unwrap();
    }

    #[test]
    fn validation_rejects_invalid_ticket() {
        let dir = tempdir().unwrap();
        write_default_schemas(dir.path()).unwrap();
        let registry = SchemaRegistry::load(dir.path()).unwrap();

        let mut ticket = Ticket::new(
            NewTicket {
                title: "Test".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        ticket.title = "".to_string();
        let err = registry.validate_ticket(&ticket).unwrap_err();
        assert!(matches!(err, TikError::Schema(_)));
    }
}
