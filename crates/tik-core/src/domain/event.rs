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

    #[test]
    fn event_type_parse_accepts_aliases_and_custom() {
        assert!(matches!(EventType::parse("create"), EventType::Created));
        assert!(matches!(EventType::parse("comment"), EventType::Note));
        assert!(matches!(
            EventType::parse("statuschange"),
            EventType::StatusChange
        ));
        assert!(matches!(
            EventType::parse("edited"),
            EventType::TicketEdited
        ));
        assert!(matches!(
            EventType::parse("relationadded"),
            EventType::RelationAdded
        ));
        assert!(matches!(
            EventType::parse("artifactremoved"),
            EventType::ArtifactRemoved
        ));
        assert!(matches!(
            EventType::parse("milestonecleared"),
            EventType::MilestoneCleared
        ));
        assert!(matches!(
            EventType::parse("assigneesadded"),
            EventType::AssigneesAdded
        ));
        assert!(matches!(
            EventType::parse("tagsremoved"),
            EventType::TagsRemoved
        ));
        match EventType::parse("custom_event") {
            EventType::Custom(name) => assert_eq!(name, "custom_event"),
            other => panic!("expected custom, got {other:?}"),
        }
    }

    #[test]
    fn actor_type_parsing_and_display() {
        assert!(matches!(ActorType::from_str("human"), Ok(ActorType::Human)));
        assert!(matches!(ActorType::from_str("user"), Ok(ActorType::Human)));
        assert!(matches!(ActorType::from_str("agent"), Ok(ActorType::Agent)));
        assert!(matches!(ActorType::from_str("ai"), Ok(ActorType::Agent)));
        assert!(matches!(ActorType::from_str("bot"), Ok(ActorType::Agent)));
        assert!(matches!(ActorType::from_str("system"), Ok(ActorType::System)));
        assert!(matches!(ActorType::from_str("auto"), Ok(ActorType::System)));
        assert!(matches!(
            ActorType::from_str("migration"),
            Ok(ActorType::System)
        ));
        assert!(matches!(
            ActorType::from_str("unknown"),
            Ok(ActorType::Human)
        ));
    }

    #[test]
    fn actor_parsing_supports_prefixes() {
        let actor = Actor::parse("agent:codex");
        assert_eq!(actor.kind, ActorType::Agent);
        assert_eq!(actor.identifier, "codex");

        let actor = Actor::parse("system");
        assert_eq!(actor.kind, ActorType::System);
        assert_eq!(actor.identifier, "system");

        let actor = Actor::parse("ai-bot");
        assert_eq!(actor.kind, ActorType::Agent);

        let actor = Actor::parse("jane");
        assert_eq!(actor.kind, ActorType::Human);
        assert_eq!(actor.to_string(), "human:jane");
    }

    #[test]
    fn status_change_event_includes_reason_when_present() {
        let event = Event::status_change(
            "human",
            "2026-01-01T00:00:00Z",
            "open",
            "closed",
            Some("done"),
        );
        assert_eq!(event.data["from"], "open");
        assert_eq!(event.data["to"], "closed");
        assert_eq!(event.data["reason"], "done");

        let event = Event::status_change(
            "human",
            "2026-01-01T00:00:00Z",
            "open",
            "closed",
            None,
        );
        assert!(event.data.get("reason").is_none());
    }

    #[test]
    fn parsed_event_type_and_actor_work() {
        let event = Event::new("status_change", "agent:codex", "2026-01-01T00:00:00Z", json!({}));
        assert!(matches!(event.event_type(), EventType::StatusChange));
        let actor = event.parsed_actor();
        assert_eq!(actor.kind, ActorType::Agent);
        assert_eq!(actor.identifier, "codex");
    }
}
