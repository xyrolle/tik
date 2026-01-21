use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::str::FromStr;

use crate::domain::ids::{MilestoneId, TicketId};
use crate::{Result, TikError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    Open,
    InProgress,
    Blocked,
    Closed,
    Archived,
}

impl TicketStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TicketStatus::Open => "open",
            TicketStatus::InProgress => "in_progress",
            TicketStatus::Blocked => "blocked",
            TicketStatus::Closed => "closed",
            TicketStatus::Archived => "archived",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        TicketStatus::from_str(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TicketType {
    Feature,
    Bug,
    Chore,
    Task,
    Spike,
}

impl TicketType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TicketType::Feature => "feature",
            TicketType::Bug => "bug",
            TicketType::Chore => "chore",
            TicketType::Task => "task",
            TicketType::Spike => "spike",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::Low => "low",
            Priority::Medium => "medium",
            Priority::High => "high",
            Priority::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Normal,
    High,
    Critical,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Low => "low",
            Severity::Normal => "normal",
            Severity::High => "high",
            Severity::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Estimate {
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    Blocks,
    BlockedBy,
    DependsOn,
    Duplicate,
    Parent,
    Child,
}

impl RelationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RelationType::Blocks => "blocks",
            RelationType::BlockedBy => "blocked_by",
            RelationType::DependsOn => "depends_on",
            RelationType::Duplicate => "duplicate",
            RelationType::Parent => "parent",
            RelationType::Child => "child",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        RelationType::from_str(value)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Relation {
    #[serde(rename = "type")]
    pub kind: RelationType,
    pub id: TicketId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    File,
    Url,
    Commit,
}

impl ArtifactType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactType::File => "file",
            ArtifactType::Url => "url",
            ArtifactType::Commit => "commit",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        ArtifactType::from_str(value)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Artifact {
    #[serde(rename = "type")]
    pub kind: ArtifactType,
    #[serde(rename = "ref")]
    pub reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ticket {
    pub schema_version: String,
    pub id: TicketId,
    pub title: String,
    pub status: TicketStatus,
    #[serde(rename = "type")]
    pub kind: TicketType,
    pub priority: Priority,
    pub severity: Severity,
    pub assignees: Vec<String>,
    pub milestone_id: Option<MilestoneId>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
    pub summary: String,
    pub description: String,
    pub acceptance: Vec<String>,
    pub estimate: Option<Estimate>,
    pub due_at: Option<String>,
    pub relations: Vec<Relation>,
    pub artifacts: Vec<Artifact>,
    pub custom: Map<String, Value>,
}

#[derive(Debug, Clone)]
pub struct NewTicket {
    pub title: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

impl Ticket {
    pub fn new(new_ticket: NewTicket, now: &str) -> Ticket {
        let description = new_ticket.description.unwrap_or_else(default_description);
        let summary = new_ticket
            .summary
            .unwrap_or_else(|| new_ticket.title.clone());

        Ticket {
            schema_version: "1.0".to_string(),
            id: TicketId::new(),
            title: new_ticket.title,
            status: TicketStatus::Open,
            kind: TicketType::Task,
            priority: Priority::Medium,
            severity: Severity::Normal,
            assignees: Vec::new(),
            milestone_id: None,
            tags: new_ticket.tags,
            created_at: now.to_string(),
            updated_at: now.to_string(),
            closed_at: None,
            summary,
            description,
            acceptance: Vec::new(),
            estimate: None,
            due_at: None,
            relations: Vec::new(),
            artifacts: Vec::new(),
            custom: Map::new(),
        }
    }

    pub fn touch(&mut self, now: &str) {
        self.updated_at = now.to_string();
    }

    pub fn set_status(&mut self, status: TicketStatus, now: &str) {
        self.status = status;
        self.updated_at = now.to_string();
        if self.status == TicketStatus::Closed {
            self.closed_at = Some(now.to_string());
        } else {
            self.closed_at = None;
        }
    }
}

impl FromStr for TicketStatus {
    type Err = TikError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "open" => Ok(TicketStatus::Open),
            "in_progress" => Ok(TicketStatus::InProgress),
            "blocked" => Ok(TicketStatus::Blocked),
            "closed" => Ok(TicketStatus::Closed),
            "archived" => Ok(TicketStatus::Archived),
            _ => Err(TikError::Schema(format!("invalid status: {s}"))),
        }
    }
}

impl FromStr for RelationType {
    type Err = TikError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "blocks" => Ok(RelationType::Blocks),
            "blocked_by" => Ok(RelationType::BlockedBy),
            "depends_on" => Ok(RelationType::DependsOn),
            "duplicate" => Ok(RelationType::Duplicate),
            "parent" => Ok(RelationType::Parent),
            "child" => Ok(RelationType::Child),
            _ => Err(TikError::Schema(format!("invalid relation type: {s}"))),
        }
    }
}

impl FromStr for ArtifactType {
    type Err = TikError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "file" => Ok(ArtifactType::File),
            "url" => Ok(ArtifactType::Url),
            "commit" => Ok(ArtifactType::Commit),
            _ => Err(TikError::Schema(format!("invalid artifact type: {s}"))),
        }
    }
}

fn default_description() -> String {
    "## Context\n\n## Requirements\n".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_ticket_defaults() {
        let ticket = Ticket::new(
            NewTicket {
                title: "Add feature".to_string(),
                summary: None,
                description: None,
                tags: vec!["mvp".to_string()],
            },
            "2026-01-01T00:00:00Z",
        );

        assert_eq!(ticket.schema_version, "1.0");
        assert!(ticket.id.as_str().starts_with("T-"));
        assert_eq!(ticket.status, TicketStatus::Open);
        assert_eq!(ticket.kind, TicketType::Task);
        assert_eq!(ticket.priority, Priority::Medium);
        assert_eq!(ticket.severity, Severity::Normal);
        assert_eq!(ticket.summary, "Add feature");
        assert!(ticket.description.contains("## Context"));
        assert_eq!(ticket.tags, vec!["mvp"]);
    }

    #[test]
    fn touch_updates_timestamp() {
        let mut ticket = Ticket::new(
            NewTicket {
                title: "Add feature".to_string(),
                summary: Some("Summary".to_string()),
                description: Some("Desc".to_string()),
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        ticket.touch("2026-01-02T00:00:00Z");
        assert_eq!(ticket.updated_at, "2026-01-02T00:00:00Z");
    }

    #[test]
    fn status_as_str_matches_schema() {
        assert_eq!(TicketStatus::InProgress.as_str(), "in_progress");
        assert_eq!(Priority::High.as_str(), "high");
        assert_eq!(Severity::Normal.as_str(), "normal");
        assert_eq!(TicketType::Bug.as_str(), "bug");
        assert_eq!(RelationType::DependsOn.as_str(), "depends_on");
        assert_eq!(ArtifactType::File.as_str(), "file");
    }

    #[test]
    fn set_status_updates_closed_at() {
        let mut ticket = Ticket::new(
            NewTicket {
                title: "Add feature".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        ticket.set_status(TicketStatus::Closed, "2026-01-02T00:00:00Z");
        assert_eq!(ticket.status, TicketStatus::Closed);
        assert_eq!(ticket.closed_at.as_deref(), Some("2026-01-02T00:00:00Z"));

        ticket.set_status(TicketStatus::Open, "2026-01-03T00:00:00Z");
        assert_eq!(ticket.status, TicketStatus::Open);
        assert!(ticket.closed_at.is_none());
    }
}
