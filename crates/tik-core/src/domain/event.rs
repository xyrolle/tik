use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::str::FromStr;

use crate::domain::ids::EventId;
use crate::{Result, TikError};

/// Standard event types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Created,
    Note,
    StatusChange,
    TicketEdited,
    NotesEdited,
    RelationAdded,
    RelationRemoved,
    ArtifactAdded,
    ArtifactRemoved,
    MilestoneSet,
    MilestoneCleared,
    AssigneesAdded,
    AssigneesRemoved,
    TagsAdded,
    TagsRemoved,
    AcceptanceToggled,
    #[serde(untagged)]
    Custom(String),
}

impl EventType {
    pub fn as_str(&self) -> &str {
        match self {
            EventType::Created => "created",
            EventType::Note => "note",
            EventType::StatusChange => "status_change",
            EventType::TicketEdited => "ticket_edited",
            EventType::NotesEdited => "notes_edited",
            EventType::RelationAdded => "relation_added",
            EventType::RelationRemoved => "relation_removed",
            EventType::ArtifactAdded => "artifact_added",
            EventType::ArtifactRemoved => "artifact_removed",
            EventType::MilestoneSet => "milestone_set",
            EventType::MilestoneCleared => "milestone_cleared",
            EventType::AssigneesAdded => "assignees_added",
            EventType::AssigneesRemoved => "assignees_removed",
            EventType::TagsAdded => "tags_added",
            EventType::TagsRemoved => "tags_removed",
            EventType::AcceptanceToggled => "acceptance_toggled",
            EventType::Custom(s) => s,
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "created" | "create" => EventType::Created,
            "note" | "comment" | "notes" => EventType::Note,
            "status_change" | "statuschange" | "status" => EventType::StatusChange,
            "ticket_edited" | "ticketedited" | "edited" | "edit" => EventType::TicketEdited,
            "notes_edited" | "notesedited" => EventType::NotesEdited,
            "relation_added" | "relationadded" => EventType::RelationAdded,
            "relation_removed" | "relationremoved" => EventType::RelationRemoved,
            "artifact_added" | "artifactadded" => EventType::ArtifactAdded,
            "artifact_removed" | "artifactremoved" => EventType::ArtifactRemoved,
            "milestone_set" | "milestoneset" => EventType::MilestoneSet,
            "milestone_cleared" | "milestonecleared" => EventType::MilestoneCleared,
            "assignees_added" | "assigneesadded" | "assigned" => EventType::AssigneesAdded,
            "assignees_removed" | "assigneesremoved" | "unassigned" => EventType::AssigneesRemoved,
            "tags_added" | "tagsadded" | "tagged" => EventType::TagsAdded,
            "tags_removed" | "tagsremoved" | "untagged" => EventType::TagsRemoved,
            "acceptance_toggled" | "acceptancetoggled" => EventType::AcceptanceToggled,
            other => EventType::Custom(other.to_string()),
        }
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Actor type for standardized actor identification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    Human,
    Agent,
    System,
}

impl ActorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorType::Human => "human",
            ActorType::Agent => "agent",
            ActorType::System => "system",
        }
    }
}

impl FromStr for ActorType {
    type Err = TikError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "human" | "user" => Ok(ActorType::Human),
            "agent" | "ai" | "bot" => Ok(ActorType::Agent),
            "system" | "auto" | "migration" => Ok(ActorType::System),
            _ => Ok(ActorType::Human), // Default to human
        }
    }
}

/// Structured actor with type and identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub kind: ActorType,
    pub identifier: String,
}

impl Actor {
    pub fn new(kind: ActorType, identifier: &str) -> Self {
        Self {
            kind,
            identifier: identifier.to_string(),
        }
    }

    pub fn human(identifier: &str) -> Self {
        Self::new(ActorType::Human, identifier)
    }

    pub fn agent(identifier: &str) -> Self {
        Self::new(ActorType::Agent, identifier)
    }

    pub fn system(identifier: &str) -> Self {
        Self::new(ActorType::System, identifier)
    }

    /// Parse an actor string. Supports "type:identifier" or just "identifier" (defaults to human).
    pub fn parse(value: &str) -> Self {
        let trimmed = value.trim();
        if let Some(idx) = trimmed.find(':') {
            let kind_str = &trimmed[..idx];
            let identifier = &trimmed[idx + 1..];
            let kind = ActorType::from_str(kind_str).unwrap_or(ActorType::Human);
            Actor::new(kind, identifier)
        } else {
            // Check for special system names
            let lower = trimmed.to_lowercase();
            if lower == "system" || lower == "migration" || lower == "auto" {
                Actor::system(&lower)
            } else if lower.starts_with("agent")
                || lower.starts_with("bot")
                || lower.starts_with("ai")
            {
                Actor::agent(trimmed)
            } else {
                Actor::human(trimmed)
            }
        }
    }

}

impl std::fmt::Display for Actor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.kind.as_str(), self.identifier)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub event_id: EventId,
    pub ts: String,
    pub actor: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub data: Value,
}

impl Event {
    pub fn new(kind: &str, actor: &str, ts: &str, data: Value) -> Event {
        Event {
            event_id: EventId::new(),
            ts: ts.to_string(),
            actor: actor.to_string(),
            kind: kind.to_string(),
            data,
        }
    }

    pub fn with_type(event_type: EventType, actor: &str, ts: &str, data: Value) -> Event {
        Event::new(event_type.as_str(), actor, ts, data)
    }

    pub fn note(actor: &str, ts: &str, text: &str) -> Event {
        Event::new("note", actor, ts, json!({"text": text}))
    }

    pub fn created(actor: &str, ts: &str, snapshot: &Value) -> Event {
        Event::new("created", actor, ts, json!({"snapshot": snapshot}))
    }

    pub fn status_change(
        actor: &str,
        ts: &str,
        from: &str,
        to: &str,
        reason: Option<&str>,
    ) -> Event {
        let mut data = json!({"from": from, "to": to});
        if let Some(reason) = reason {
            data["reason"] = json!(reason);
        }
        Event::new("status_change", actor, ts, data)
    }

    pub fn acceptance_toggled(actor: &str, ts: &str, index: usize, completed: bool) -> Event {
        Event::new(
            "acceptance_toggled",
            actor,
            ts,
            json!({"index": index, "completed": completed}),
        )
    }

    /// Get the event type as an enum.
    pub fn event_type(&self) -> EventType {
        EventType::parse(&self.kind)
    }

    /// Get the actor as a structured Actor.
    pub fn parsed_actor(&self) -> Actor {
        Actor::parse(&self.actor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_event_sets_kind_and_data() {
        let event = Event::note("human", "2026-01-01T00:00:00Z", "hello");
        assert_eq!(event.kind, "note");
        assert_eq!(event.actor, "human");
        assert_eq!(event.data["text"], "hello");
    }
}
