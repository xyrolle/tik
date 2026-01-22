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

    pub fn parse(value: &str) -> Result<Self> {
        TicketType::from_str(value)
    }
}

impl FromStr for TicketType {
    type Err = TikError;

    fn from_str(s: &str) -> Result<Self> {
        let lower = s.trim().to_lowercase();
        match lower.as_str() {
            "feature" => Ok(TicketType::Feature),
            "bug" => Ok(TicketType::Bug),
            "chore" => Ok(TicketType::Chore),
            "task" => Ok(TicketType::Task),
            "spike" => Ok(TicketType::Spike),
            _ => Err(TikError::Schema(format!("invalid ticket type: {s}"))),
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

    pub fn parse(value: &str) -> Result<Self> {
        Priority::from_str(value)
    }
}

impl FromStr for Priority {
    type Err = TikError;

    fn from_str(s: &str) -> Result<Self> {
        let lower = s.trim().to_lowercase();
        match lower.as_str() {
            "low" => Ok(Priority::Low),
            "medium" => Ok(Priority::Medium),
            "high" => Ok(Priority::High),
            "critical" => Ok(Priority::Critical),
            _ => Err(TikError::Schema(format!("invalid priority: {s}"))),
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

    pub fn parse(value: &str) -> Result<Self> {
        Severity::from_str(value)
    }
}

impl FromStr for Severity {
    type Err = TikError;

    fn from_str(s: &str) -> Result<Self> {
        let lower = s.trim().to_lowercase();
        match lower.as_str() {
            "low" => Ok(Severity::Low),
            "normal" => Ok(Severity::Normal),
            "high" => Ok(Severity::High),
            "critical" => Ok(Severity::Critical),
            _ => Err(TikError::Schema(format!("invalid severity: {s}"))),
        }
    }
}

/// Estimate unit enum for structured estimation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EstimateUnit {
    Hours,
    Days,
    Weeks,
    #[default]
    Points,
    StoryPoints,
}

impl EstimateUnit {
    pub fn as_str(&self) -> &'static str {
        match self {
            EstimateUnit::Hours => "hours",
            EstimateUnit::Days => "days",
            EstimateUnit::Weeks => "weeks",
            EstimateUnit::Points => "points",
            EstimateUnit::StoryPoints => "story_points",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        EstimateUnit::from_str(value)
    }
}

impl FromStr for EstimateUnit {
    type Err = TikError;

    fn from_str(s: &str) -> Result<Self> {
        let lower = s.to_lowercase();
        match lower.as_str() {
            "hours" | "h" | "hour" | "hr" | "hrs" => Ok(EstimateUnit::Hours),
            "days" | "d" | "day" => Ok(EstimateUnit::Days),
            "weeks" | "w" | "week" | "wk" | "wks" => Ok(EstimateUnit::Weeks),
            "points" | "p" | "pt" | "pts" | "point" => Ok(EstimateUnit::Points),
            "story_points" | "sp" | "storypoint" | "storypoints" => Ok(EstimateUnit::StoryPoints),
            _ => Err(TikError::Schema(format!("invalid estimate unit: {s}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Estimate {
    pub value: f64,
    /// The unit field supports both string (legacy) and EstimateUnit enum.
    /// For backward compatibility, we serialize/deserialize as string.
    pub unit: String,
}

impl Estimate {
    pub fn new(value: f64, unit: EstimateUnit) -> Self {
        Self {
            value,
            unit: unit.as_str().to_string(),
        }
    }

    pub fn unit_enum(&self) -> Result<EstimateUnit> {
        EstimateUnit::parse(&self.unit)
    }
}

/// Acceptance criterion with completion tracking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum AcceptanceCriterion {
    /// Legacy format: just a string.
    Simple(String),
    /// New format: structured with completion tracking.
    Structured {
        text: String,
        completed: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        completed_at: Option<String>,
    },
}

impl AcceptanceCriterion {
    pub fn new(text: String) -> Self {
        AcceptanceCriterion::Structured {
            text,
            completed: false,
            completed_at: None,
        }
    }

    pub fn text(&self) -> &str {
        match self {
            AcceptanceCriterion::Simple(s) => s,
            AcceptanceCriterion::Structured { text, .. } => text,
        }
    }

    pub fn is_completed(&self) -> bool {
        match self {
            AcceptanceCriterion::Simple(_) => false,
            AcceptanceCriterion::Structured { completed, .. } => *completed,
        }
    }

    pub fn toggle(&mut self, now: &str) {
        match self {
            AcceptanceCriterion::Simple(text) => {
                *self = AcceptanceCriterion::Structured {
                    text: text.clone(),
                    completed: true,
                    completed_at: Some(now.to_string()),
                };
            }
            AcceptanceCriterion::Structured {
                completed,
                completed_at,
                ..
            } => {
                *completed = !*completed;
                *completed_at = if *completed {
                    Some(now.to_string())
                } else {
                    None
                };
            }
        }
    }

    /// Convert a simple criterion to structured format.
    pub fn to_structured(&self) -> AcceptanceCriterion {
        match self {
            AcceptanceCriterion::Simple(text) => AcceptanceCriterion::Structured {
                text: text.clone(),
                completed: false,
                completed_at: None,
            },
            AcceptanceCriterion::Structured { .. } => self.clone(),
        }
    }
}

/// Normalize a tag to slug format: lowercase, hyphens only, no leading/trailing hyphens.
pub fn normalize_tag(tag: &str) -> String {
    tag.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Normalize an assignee: lowercase, trimmed.
pub fn normalize_assignee(assignee: &str) -> String {
    assignee.trim().to_lowercase()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
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
    Pr,
}

impl ArtifactType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactType::File => "file",
            ArtifactType::Url => "url",
            ArtifactType::Commit => "commit",
            ArtifactType::Pr => "pr",
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

#[derive(Debug, Clone)]
pub struct TicketDefaults {
    pub kind: TicketType,
    pub priority: Priority,
    pub severity: Severity,
}

impl Default for TicketDefaults {
    fn default() -> Self {
        Self {
            kind: TicketType::Task,
            priority: Priority::Medium,
            severity: Severity::Normal,
        }
    }
}

impl Ticket {
    pub fn new(new_ticket: NewTicket, now: &str) -> Ticket {
        Ticket::new_with_defaults(new_ticket, now, &TicketDefaults::default())
    }

    pub fn new_with_defaults(
        new_ticket: NewTicket,
        now: &str,
        defaults: &TicketDefaults,
    ) -> Ticket {
        let description = new_ticket.description.unwrap_or_else(default_description);
        let summary = new_ticket
            .summary
            .unwrap_or_else(|| new_ticket.title.clone());

        Ticket {
            schema_version: "1.0".to_string(),
            id: TicketId::new(),
            title: new_ticket.title,
            status: TicketStatus::Open,
            kind: defaults.kind.clone(),
            priority: defaults.priority.clone(),
            severity: defaults.severity.clone(),
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
            "pr" => Ok(ArtifactType::Pr),
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
        assert_eq!(ArtifactType::Pr.as_str(), "pr");
    }

    #[test]
    fn parse_ticket_enums_accepts_known_values() {
        assert!(matches!(TicketType::parse("Feature"), Ok(TicketType::Feature)));
        assert!(matches!(Priority::parse("HIGH"), Ok(Priority::High)));
        assert!(matches!(Severity::parse("normal"), Ok(Severity::Normal)));
        assert!(matches!(ArtifactType::parse("pr"), Ok(ArtifactType::Pr)));
        assert!(TicketType::parse("nope").is_err());
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
