use serde::{Deserialize, Serialize};

use crate::domain::ids::MilestoneId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MilestoneStatus {
    Open,
    Closed,
    Archived,
}

impl MilestoneStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MilestoneStatus::Open => "open",
            MilestoneStatus::Closed => "closed",
            MilestoneStatus::Archived => "archived",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Milestone {
    pub schema_version: String,
    pub id: MilestoneId,
    pub title: String,
    pub status: MilestoneStatus,
    pub created_at: String,
    pub updated_at: String,
    pub due_at: Option<String>,
    pub description: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewMilestone {
    pub title: String,
    pub description: Option<String>,
    pub due_at: Option<String>,
    pub tags: Vec<String>,
}

impl Milestone {
    pub fn new(new_milestone: NewMilestone, now: &str) -> Milestone {
        Milestone {
            schema_version: "1.0".to_string(),
            id: MilestoneId::new(),
            title: new_milestone.title,
            status: MilestoneStatus::Open,
            created_at: now.to_string(),
            updated_at: now.to_string(),
            due_at: new_milestone.due_at,
            description: new_milestone.description.unwrap_or_default(),
            tags: new_milestone.tags,
        }
    }

    pub fn touch(&mut self, now: &str) {
        self.updated_at = now.to_string();
    }

    pub fn set_status(&mut self, status: MilestoneStatus, now: &str) {
        self.status = status;
        self.updated_at = now.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn milestone_status_as_str_matches_schema() {
        assert_eq!(MilestoneStatus::Open.as_str(), "open");
        assert_eq!(MilestoneStatus::Closed.as_str(), "closed");
        assert_eq!(MilestoneStatus::Archived.as_str(), "archived");
    }

    #[test]
    fn new_milestone_defaults() {
        let milestone = Milestone::new(
            NewMilestone {
                title: "Phase 1".to_string(),
                description: None,
                due_at: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        assert_eq!(milestone.status, MilestoneStatus::Open);
        assert_eq!(milestone.description, "");
    }
}
