use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::{json, Value};

use crate::domain::event::Event;
use crate::domain::ids::{MilestoneId, TicketId};
use crate::domain::ticket::{
    Artifact, ArtifactType, NewTicket, Relation, RelationType, Ticket, TicketStatus,
};
use crate::fs as tikfs;
use crate::schema::SchemaRegistry;
use crate::store::event_store::EventStore;
use crate::timeutil;
use crate::{Result, TikError};

#[derive(Debug, Clone)]
pub struct TicketStore {
    data_root: PathBuf,
    schema_dir: PathBuf,
}

impl TicketStore {
    pub fn new(data_root: PathBuf, schema_dir: PathBuf) -> TicketStore {
        TicketStore {
            data_root,
            schema_dir,
        }
    }

    pub fn create(&self, new_ticket: NewTicket, actor: &str) -> Result<Ticket> {
        let now = timeutil::now_rfc3339()?;
        let ticket = Ticket::new(new_ticket, &now);
        let ticket_dir = self.ticket_dir(&ticket.id);

        if ticket_dir.exists() {
            return Err(TikError::Conflict("ticket directory exists".to_string()));
        }

        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;

        tikfs::ensure_dir(&ticket_dir)?;
        tikfs::ensure_dir(&ticket_dir.join("artifacts"))?;

        let event_store = EventStore::new(ticket_dir.join("notes.jsonl"));
        let created_event = Event::new(
            "created",
            actor,
            &now,
            json!({
                "title": ticket.title.clone(),
                "summary": ticket.summary.clone(),
                "ticket": ticket.clone()
            }),
        );
        schemas.validate_event(&created_event)?;
        event_store.append(&created_event)?;

        let ticket_path = ticket_dir.join("ticket.json");
        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_md = format!("# {} {}\n\n{}\n", ticket.id, ticket.title, ticket.summary);
        tikfs::write_string_atomic(&ticket_dir.join("notes.md"), &notes_md)?;

        Ok(ticket)
    }

    pub fn import_ticket(
        &self,
        ticket: Ticket,
        events: Vec<Event>,
        notes_md: Option<String>,
        actor: &str,
    ) -> Result<Ticket> {
        let ticket_dir = self.ticket_dir(&ticket.id);
        if ticket_dir.exists() {
            return Err(TikError::Conflict("ticket directory exists".to_string()));
        }

        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;

        tikfs::ensure_dir(&ticket_dir)?;
        tikfs::ensure_dir(&ticket_dir.join("artifacts"))?;

        let mut events = events;
        if events
            .iter()
            .all(|event| event.data.get("ticket").is_none())
        {
            let snapshot = Event::new(
                "imported",
                actor,
                &ticket.created_at,
                json!({"ticket": ticket.clone()}),
            );
            events.insert(0, snapshot);
        }
        write_events_jsonl(&ticket_dir.join("notes.jsonl"), &events, &schemas)?;

        let ticket_path = ticket_dir.join("ticket.json");
        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        if let Some(notes_md) = notes_md {
            tikfs::write_string_atomic(&notes_path, &notes_md)?;
        } else {
            ensure_notes_md(&ticket, &notes_path)?;
        }

        Ok(ticket)
    }

    pub fn load(&self, id: &TicketId) -> Result<Ticket> {
        let ticket_path = self.ticket_dir(id).join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let raw = tikfs::read_to_string(&ticket_path)?;
        let ticket: Ticket = serde_json::from_str(&raw)
            .map_err(|err| TikError::Schema(format!("invalid ticket json: {err}")))?;
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        Ok(ticket)
    }

    pub fn read_raw(&self, id: &TicketId) -> Result<String> {
        let ticket_path = self.ticket_dir(id).join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }
        tikfs::read_to_string(&ticket_path)
    }

    pub fn read_notes_md(&self, id: &TicketId) -> Result<String> {
        let ticket = self.load(id)?;
        let notes_path = self.ticket_dir(id).join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        tikfs::read_to_string(&notes_path)
    }

    pub fn read_notes_md_if_exists(&self, id: &TicketId) -> Result<String> {
        let notes_path = self.ticket_dir(id).join("notes.md");
        if !notes_path.is_file() {
            return Ok(String::new());
        }
        tikfs::read_to_string(&notes_path)
    }

    pub fn write_notes_md(&self, id: &TicketId, contents: &str) -> Result<()> {
        let ticket = self.load(id)?;
        let notes_path = self.ticket_dir(id).join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        tikfs::write_string_atomic(&notes_path, contents)?;
        Ok(())
    }

    pub fn list(&self, status: Option<TicketStatus>) -> Result<Vec<Ticket>> {
        let mut tickets = Vec::new();
        let tickets_dir = self.data_root.join("tickets");
        if !tickets_dir.exists() {
            return Ok(tickets);
        }
        let schemas = SchemaRegistry::load(&self.schema_dir())?;

        for entry in
            fs::read_dir(&tickets_dir).map_err(|err| TikError::io("read tickets dir", err))?
        {
            let entry = entry.map_err(|err| TikError::io("read tickets dir", err))?;
            if !entry
                .file_type()
                .map_err(|err| TikError::io("read dir entry", err))?
                .is_dir()
            {
                continue;
            }

            let ticket_path = entry.path().join("ticket.json");
            if !ticket_path.is_file() {
                continue;
            }

            let raw = tikfs::read_to_string(&ticket_path)?;
            let ticket: Ticket = serde_json::from_str(&raw)
                .map_err(|err| TikError::Schema(format!("invalid ticket json: {err}")))?;
            schemas.validate_ticket(&ticket)?;
            if let Some(ref filter) = status {
                if &ticket.status != filter {
                    continue;
                }
            }
            tickets.push(ticket);
        }

        tickets.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        Ok(tickets)
    }

    pub fn list_page(
        &self,
        status: Option<TicketStatus>,
        offset: usize,
        limit: Option<usize>,
    ) -> Result<Vec<Ticket>> {
        let tickets = self.list(status)?;
        let iter = tickets.into_iter().skip(offset);
        let page: Vec<Ticket> = match limit {
            Some(limit) => iter.take(limit).collect(),
            None => iter.collect(),
        };
        Ok(page)
    }

    pub fn append_note(&self, id: &TicketId, actor: &str, text: &str) -> Result<Event> {
        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let now = timeutil::now_rfc3339()?;
        let mut ticket = self.load(id)?;
        ticket.touch(&now);
        let event = Event::note(actor, &now, text);
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        append_notes_md(&notes_path, &now, actor, text)?;

        Ok(event)
    }

    pub fn update_status(
        &self,
        id: &TicketId,
        new_status: TicketStatus,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let now = timeutil::now_rfc3339()?;
        let mut ticket = self.load(id)?;
        let from_status = ticket.status.clone();
        if from_status == new_status {
            return Ok(ticket);
        }

        ticket.set_status(new_status.clone(), &now);
        let data = if let Some(reason) = reason {
            json!({"from": from_status.as_str(), "to": new_status.as_str(), "reason": reason})
        } else {
            json!({"from": from_status.as_str(), "to": new_status.as_str()})
        };

        let event = Event::new("status_change", actor, &now, data);
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        let mut line = format!(
            "status_change {} -> {}",
            from_status.as_str(),
            new_status.as_str()
        );
        if let Some(reason) = reason {
            line.push_str(&format!(" ({reason})"));
        }
        append_notes_md(&notes_path, &now, actor, &line)?;

        Ok(ticket)
    }

    pub fn add_relation(
        &self,
        id: &TicketId,
        kind: RelationType,
        target: &TicketId,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        if id == target {
            return Err(TikError::usage("cannot relate ticket to itself"));
        }

        if !self.ticket_dir(target).join("ticket.json").is_file() {
            return Err(TikError::NotFound(format!("ticket {}", target.as_str())));
        }

        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let now = timeutil::now_rfc3339()?;
        let mut ticket = self.load(id)?;
        let exists = ticket
            .relations
            .iter()
            .any(|relation| relation.kind == kind && relation.id == *target);
        if exists {
            return Ok(ticket);
        }

        ticket.relations.push(Relation {
            kind: kind.clone(),
            id: target.clone(),
        });
        ticket
            .relations
            .sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        ticket.touch(&now);

        let data = if let Some(reason) = reason {
            json!({"type": kind.as_str(), "id": target.as_str(), "reason": reason})
        } else {
            json!({"type": kind.as_str(), "id": target.as_str()})
        };
        let event = Event::new("relation_added", actor, &now, data);
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        let mut line = format!("relation_added {} {}", kind.as_str(), target.as_str());
        if let Some(reason) = reason {
            line.push_str(&format!(" ({reason})"));
        }
        append_notes_md(&notes_path, &now, actor, &line)?;

        Ok(ticket)
    }

    pub fn remove_relation(
        &self,
        id: &TicketId,
        kind: RelationType,
        target: &TicketId,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let now = timeutil::now_rfc3339()?;
        let mut ticket = self.load(id)?;
        let original_len = ticket.relations.len();
        ticket
            .relations
            .retain(|relation| !(relation.kind == kind && relation.id == *target));
        if ticket.relations.len() == original_len {
            return Ok(ticket);
        }
        ticket.touch(&now);

        let data = if let Some(reason) = reason {
            json!({"type": kind.as_str(), "id": target.as_str(), "reason": reason})
        } else {
            json!({"type": kind.as_str(), "id": target.as_str()})
        };
        let event = Event::new("relation_removed", actor, &now, data);
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        let mut line = format!("relation_removed {} {}", kind.as_str(), target.as_str());
        if let Some(reason) = reason {
            line.push_str(&format!(" ({reason})"));
        }
        append_notes_md(&notes_path, &now, actor, &line)?;

        Ok(ticket)
    }

    pub fn add_artifact(
        &self,
        id: &TicketId,
        kind: ArtifactType,
        reference: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        validate_artifact_ref(&kind, reference)?;

        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let now = timeutil::now_rfc3339()?;
        let mut ticket = self.load(id)?;
        let exists = ticket
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == kind && artifact.reference == reference);
        if exists {
            return Ok(ticket);
        }

        ticket.artifacts.push(Artifact {
            kind: kind.clone(),
            reference: reference.to_string(),
        });
        ticket
            .artifacts
            .sort_by(|a, b| a.reference.cmp(&b.reference));
        ticket.touch(&now);

        let data = if let Some(reason) = reason {
            json!({"type": kind.as_str(), "ref": reference, "reason": reason})
        } else {
            json!({"type": kind.as_str(), "ref": reference})
        };
        let event = Event::new("artifact_added", actor, &now, data);
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        let mut line = format!("artifact_added {} {}", kind.as_str(), reference);
        if let Some(reason) = reason {
            line.push_str(&format!(" ({reason})"));
        }
        append_notes_md(&notes_path, &now, actor, &line)?;

        Ok(ticket)
    }

    pub fn remove_artifact(
        &self,
        id: &TicketId,
        kind: ArtifactType,
        reference: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let now = timeutil::now_rfc3339()?;
        let mut ticket = self.load(id)?;
        let original_len = ticket.artifacts.len();
        ticket
            .artifacts
            .retain(|artifact| !(artifact.kind == kind && artifact.reference == reference));
        if ticket.artifacts.len() == original_len {
            return Ok(ticket);
        }
        ticket.touch(&now);

        let data = if let Some(reason) = reason {
            json!({"type": kind.as_str(), "ref": reference, "reason": reason})
        } else {
            json!({"type": kind.as_str(), "ref": reference})
        };
        let event = Event::new("artifact_removed", actor, &now, data);
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        let mut line = format!("artifact_removed {} {}", kind.as_str(), reference);
        if let Some(reason) = reason {
            line.push_str(&format!(" ({reason})"));
        }
        append_notes_md(&notes_path, &now, actor, &line)?;

        Ok(ticket)
    }

    pub fn apply_edit(
        &self,
        id: &TicketId,
        raw_json: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let existing = self.load(id)?;
        let mut edited: Ticket = serde_json::from_str(raw_json)
            .map_err(|err| TikError::Schema(format!("invalid ticket json: {err}")))?;

        if edited.id != *id {
            return Err(TikError::Schema("ticket id mismatch".to_string()));
        }

        edited.schema_version = existing.schema_version.clone();
        edited.created_at = existing.created_at.clone();

        let now = timeutil::now_rfc3339()?;
        edited.updated_at = now.clone();
        if edited.status != TicketStatus::Closed {
            edited.closed_at = None;
        } else if edited.closed_at.is_none() {
            edited.closed_at = Some(now.clone());
        }

        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&edited)?;
        let data = if let Some(reason) = reason {
            json!({"reason": reason, "ticket": edited.clone()})
        } else {
            json!({"ticket": edited.clone()})
        };
        let event = Event::new("ticket_edited", actor, &now, data);
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&edited)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        ensure_notes_md(&edited, &notes_path)?;
        let mut line = "ticket_edited".to_string();
        if let Some(reason) = reason {
            line.push_str(&format!(" ({reason})"));
        }
        append_notes_md(&notes_path, &now, actor, &line)?;

        Ok(edited)
    }

    pub fn touch_notes(&self, id: &TicketId, actor: &str, reason: Option<&str>) -> Result<Event> {
        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let now = timeutil::now_rfc3339()?;
        let mut ticket = self.load(id)?;
        ticket.touch(&now);
        let data = if let Some(reason) = reason {
            json!({"reason": reason})
        } else {
            json!({})
        };
        let event = Event::new("notes_edited", actor, &now, data);
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        Ok(event)
    }

    pub fn set_milestone(
        &self,
        id: &TicketId,
        milestone_id: Option<&MilestoneId>,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        if let Some(milestone_id) = milestone_id {
            let milestone_path = self.milestone_path(milestone_id);
            if !milestone_path.is_file() {
                return Err(TikError::NotFound(format!(
                    "milestone {}",
                    milestone_id.as_str()
                )));
            }
        }

        let now = timeutil::now_rfc3339()?;
        let mut ticket = self.load(id)?;
        ticket.milestone_id = milestone_id.cloned();
        ticket.touch(&now);

        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;

        let data = match milestone_id {
            Some(milestone_id) => {
                if let Some(reason) = reason {
                    json!({"milestone_id": milestone_id.as_str(), "reason": reason})
                } else {
                    json!({"milestone_id": milestone_id.as_str()})
                }
            }
            None => {
                if let Some(reason) = reason {
                    json!({"milestone_id": null, "reason": reason})
                } else {
                    json!({"milestone_id": null})
                }
            }
        };
        let kind = if milestone_id.is_some() {
            "milestone_set"
        } else {
            "milestone_cleared"
        };
        let event = Event::new(kind, actor, &now, data);
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        let mut line = if let Some(milestone_id) = milestone_id {
            format!("milestone_set {}", milestone_id.as_str())
        } else {
            "milestone_cleared".to_string()
        };
        if let Some(reason) = reason {
            line.push_str(&format!(" ({reason})"));
        }
        append_notes_md(&notes_path, &now, actor, &line)?;

        Ok(ticket)
    }

    pub fn add_assignees(
        &self,
        id: &TicketId,
        assignees: Vec<String>,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let assignees = normalize_values(assignees, "assignee", false)?;

        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let now = timeutil::now_rfc3339()?;
        let mut ticket = self.load(id)?;
        let mut added = Vec::new();
        for assignee in assignees {
            if !ticket.assignees.contains(&assignee) {
                ticket.assignees.push(assignee.clone());
                added.push(assignee);
            }
        }
        if added.is_empty() {
            return Ok(ticket);
        }
        ticket.assignees.sort();
        ticket.touch(&now);

        let data = if let Some(reason) = reason {
            json!({"added": added, "reason": reason})
        } else {
            json!({"added": added})
        };
        let event = Event::new("assignees_added", actor, &now, data);
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        let line = format!("assignees_added {}", ticket.assignees.join(", "));
        append_notes_md(&notes_path, &now, actor, &line)?;

        Ok(ticket)
    }

    pub fn remove_assignees(
        &self,
        id: &TicketId,
        assignees: Vec<String>,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let assignees = normalize_values(assignees, "assignee", false)?;

        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let now = timeutil::now_rfc3339()?;
        let mut ticket = self.load(id)?;
        let mut removed = Vec::new();
        for assignee in &assignees {
            if ticket.assignees.contains(assignee) {
                removed.push(assignee.clone());
            }
        }
        if removed.is_empty() {
            return Ok(ticket);
        }
        ticket
            .assignees
            .retain(|assignee| !assignees.contains(assignee));
        ticket.touch(&now);

        let data = if let Some(reason) = reason {
            json!({"removed": removed, "reason": reason})
        } else {
            json!({"removed": removed})
        };
        let event = Event::new("assignees_removed", actor, &now, data);
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        let line = format!("assignees_removed {}", removed.join(", "));
        append_notes_md(&notes_path, &now, actor, &line)?;

        Ok(ticket)
    }

    pub fn set_assignees(
        &self,
        id: &TicketId,
        assignees: Vec<String>,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let assignees = normalize_values(assignees, "assignee", true)?;

        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let now = timeutil::now_rfc3339()?;
        let mut ticket = self.load(id)?;
        if ticket.assignees == assignees {
            return Ok(ticket);
        }
        let from = ticket.assignees.clone();
        ticket.assignees = assignees;
        ticket.touch(&now);

        let data = if let Some(reason) = reason {
            json!({"from": from, "to": ticket.assignees, "reason": reason})
        } else {
            json!({"from": from, "to": ticket.assignees})
        };
        let event = Event::new("assignees_set", actor, &now, data);
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        let line = if ticket.assignees.is_empty() {
            "assignees_cleared".to_string()
        } else {
            format!("assignees_set {}", ticket.assignees.join(", "))
        };
        append_notes_md(&notes_path, &now, actor, &line)?;

        Ok(ticket)
    }

    pub fn add_tags(
        &self,
        id: &TicketId,
        tags: Vec<String>,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let tags = normalize_values(tags, "tag", false)?;

        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let now = timeutil::now_rfc3339()?;
        let mut ticket = self.load(id)?;
        let mut added = Vec::new();
        for tag in tags {
            if !ticket.tags.contains(&tag) {
                ticket.tags.push(tag.clone());
                added.push(tag);
            }
        }
        if added.is_empty() {
            return Ok(ticket);
        }
        ticket.tags.sort();
        ticket.touch(&now);

        let data = if let Some(reason) = reason {
            json!({"added": added, "reason": reason})
        } else {
            json!({"added": added})
        };
        let event = Event::new("tags_added", actor, &now, data);
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        let line = format!("tags_added {}", ticket.tags.join(", "));
        append_notes_md(&notes_path, &now, actor, &line)?;

        Ok(ticket)
    }

    pub fn remove_tags(
        &self,
        id: &TicketId,
        tags: Vec<String>,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let tags = normalize_values(tags, "tag", false)?;

        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let now = timeutil::now_rfc3339()?;
        let mut ticket = self.load(id)?;
        let mut removed = Vec::new();
        for tag in &tags {
            if ticket.tags.contains(tag) {
                removed.push(tag.clone());
            }
        }
        if removed.is_empty() {
            return Ok(ticket);
        }
        ticket.tags.retain(|tag| !tags.contains(tag));
        ticket.touch(&now);

        let data = if let Some(reason) = reason {
            json!({"removed": removed, "reason": reason})
        } else {
            json!({"removed": removed})
        };
        let event = Event::new("tags_removed", actor, &now, data);
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        let line = format!("tags_removed {}", removed.join(", "));
        append_notes_md(&notes_path, &now, actor, &line)?;

        Ok(ticket)
    }

    pub fn set_tags(
        &self,
        id: &TicketId,
        tags: Vec<String>,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let tags = normalize_values(tags, "tag", true)?;

        let ticket_dir = self.ticket_dir(id);
        let ticket_path = ticket_dir.join("ticket.json");
        if !ticket_path.is_file() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }

        let now = timeutil::now_rfc3339()?;
        let mut ticket = self.load(id)?;
        if ticket.tags == tags {
            return Ok(ticket);
        }
        let from = ticket.tags.clone();
        ticket.tags = tags;
        ticket.touch(&now);

        let data = if let Some(reason) = reason {
            json!({"from": from, "to": ticket.tags, "reason": reason})
        } else {
            json!({"from": from, "to": ticket.tags})
        };
        let event = Event::new("tags_set", actor, &now, data);
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        schemas.validate_event(&event)?;
        EventStore::new(ticket_dir.join("notes.jsonl")).append(&event)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        tikfs::write_string_atomic(&ticket_path, &ticket_json)?;

        let notes_path = ticket_dir.join("notes.md");
        ensure_notes_md(&ticket, &notes_path)?;
        let line = if ticket.tags.is_empty() {
            "tags_cleared".to_string()
        } else {
            format!("tags_set {}", ticket.tags.join(", "))
        };
        append_notes_md(&notes_path, &now, actor, &line)?;

        Ok(ticket)
    }

    pub fn read_events(&self, id: &TicketId) -> Result<Vec<Event>> {
        let ticket_dir = self.ticket_dir(id);
        if !ticket_dir.is_dir() {
            return Err(TikError::NotFound(format!("ticket {}", id.as_str())));
        }
        let events = EventStore::new(ticket_dir.join("notes.jsonl")).read_all()?;
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        for event in &events {
            schemas.validate_event(event)?;
        }
        Ok(events)
    }

    pub fn rebuild_from_events(&self, id: &TicketId, events: &[Event]) -> Result<Ticket> {
        let mut snapshot_idx = None;
        for (idx, event) in events.iter().enumerate() {
            if (event.kind == "created"
                || event.kind == "ticket_edited"
                || event.kind == "imported")
                && event.data.get("ticket").is_some()
            {
                snapshot_idx = Some(idx);
            }
        }

        let snapshot_idx =
            snapshot_idx.ok_or_else(|| TikError::Schema("missing ticket snapshot".to_string()))?;
        let snapshot = extract_ticket_snapshot(&events[snapshot_idx])?;
        if snapshot.id != *id {
            return Err(TikError::Schema(
                "ticket id mismatch in snapshot".to_string(),
            ));
        }

        let mut ticket = snapshot;
        for event in events.iter().skip(snapshot_idx + 1) {
            apply_event(&mut ticket, event)?;
        }

        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_ticket(&ticket)?;
        Ok(ticket)
    }

    fn schema_dir(&self) -> PathBuf {
        self.schema_dir.clone()
    }

    fn ticket_dir(&self, id: &TicketId) -> PathBuf {
        self.data_root.join("tickets").join(id.as_str())
    }

    fn milestone_path(&self, id: &MilestoneId) -> PathBuf {
        self.data_root
            .join("milestones")
            .join(format!("{}.json", id.as_str()))
    }
}

fn append_notes_md(path: &Path, ts: &str, actor: &str, text: &str) -> Result<()> {
    let line = format!("- [{}] {}: {}\n", ts, actor, text);
    tikfs::append_string_atomic(path, &line)?;
    Ok(())
}

fn write_events_jsonl(path: &Path, events: &[Event], schemas: &SchemaRegistry) -> Result<()> {
    let mut lines = Vec::new();
    for event in events {
        schemas.validate_event(event)?;
        let line = serde_json::to_string(event)
            .map_err(|err| TikError::Schema(format!("serialize event: {err}")))?;
        lines.push(line);
    }
    let mut content = lines.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }
    tikfs::write_string_atomic(path, &content)?;
    Ok(())
}

fn ensure_notes_md(ticket: &Ticket, path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let header = format!("# {} {}\n\n{}\n", ticket.id, ticket.title, ticket.summary);
    tikfs::write_string_atomic(path, &header)
}

fn validate_artifact_ref(kind: &ArtifactType, reference: &str) -> Result<()> {
    if reference.trim().is_empty() {
        return Err(TikError::usage("artifact ref cannot be empty"));
    }
    if matches!(kind, ArtifactType::File) {
        let path = Path::new(reference);
        if path.is_absolute() {
            return Err(TikError::usage("artifact file ref must be relative"));
        }
        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(TikError::usage("artifact file ref cannot contain '..'"));
        }
    }
    Ok(())
}

fn extract_ticket_snapshot(event: &Event) -> Result<Ticket> {
    let value = event
        .data
        .get("ticket")
        .ok_or_else(|| TikError::Schema("ticket snapshot missing".to_string()))?;
    serde_json::from_value(value.clone())
        .map_err(|err| TikError::Schema(format!("invalid ticket snapshot: {err}")))
}

fn extract_str<'a>(data: &'a Value, key: &str) -> Result<&'a str> {
    data.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| TikError::Schema(format!("event data missing {key}")))
}

fn extract_optional_str<'a>(data: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match data.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.as_str())),
        Some(_) => Err(TikError::Schema(format!(
            "event data {key} must be string or null"
        ))),
        None => Err(TikError::Schema(format!("event data missing {key}"))),
    }
}

fn extract_string_list(data: &Value, key: &str) -> Result<Vec<String>> {
    let list = data
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| TikError::Schema(format!("event data missing {key}")))?;

    let mut out = Vec::with_capacity(list.len());
    for value in list {
        let value = value
            .as_str()
            .ok_or_else(|| TikError::Schema(format!("event data {key} must be a string list")))?;
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            out.push(trimmed.to_string());
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn apply_event(ticket: &mut Ticket, event: &Event) -> Result<()> {
    match event.kind.as_str() {
        "created" | "imported" => Ok(()),
        "note" | "notes_edited" => {
            ticket.touch(&event.ts);
            Ok(())
        }
        "ticket_edited" => {
            let snapshot = extract_ticket_snapshot(event)?;
            if snapshot.id != ticket.id {
                return Err(TikError::Schema(
                    "ticket id mismatch in edit snapshot".to_string(),
                ));
            }
            *ticket = snapshot;
            Ok(())
        }
        "status_change" => {
            let status = TicketStatus::parse(extract_str(&event.data, "to")?)?;
            ticket.set_status(status, &event.ts);
            Ok(())
        }
        "milestone_set" => {
            let milestone_id =
                extract_optional_str(&event.data, "milestone_id")?.ok_or_else(|| {
                    TikError::Schema("milestone_set missing milestone_id".to_string())
                })?;
            ticket.milestone_id = Some(MilestoneId::parse(milestone_id)?);
            ticket.touch(&event.ts);
            Ok(())
        }
        "milestone_cleared" => {
            let milestone_id = extract_optional_str(&event.data, "milestone_id")?;
            if milestone_id.is_some() {
                return Err(TikError::Schema(
                    "milestone_cleared must have null milestone_id".to_string(),
                ));
            }
            ticket.milestone_id = None;
            ticket.touch(&event.ts);
            Ok(())
        }
        "relation_added" => {
            let kind = RelationType::parse(extract_str(&event.data, "type")?)?;
            let target = TicketId::parse(extract_str(&event.data, "id")?)?;
            if !ticket
                .relations
                .iter()
                .any(|relation| relation.kind == kind && relation.id == target)
            {
                ticket.relations.push(Relation {
                    kind: kind.clone(),
                    id: target,
                });
                ticket
                    .relations
                    .sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
            }
            ticket.touch(&event.ts);
            Ok(())
        }
        "relation_removed" => {
            let kind = RelationType::parse(extract_str(&event.data, "type")?)?;
            let target = TicketId::parse(extract_str(&event.data, "id")?)?;
            ticket
                .relations
                .retain(|relation| !(relation.kind == kind && relation.id == target));
            ticket.touch(&event.ts);
            Ok(())
        }
        "artifact_added" => {
            let kind = ArtifactType::parse(extract_str(&event.data, "type")?)?;
            let reference = extract_str(&event.data, "ref")?.to_string();
            if !ticket
                .artifacts
                .iter()
                .any(|artifact| artifact.kind == kind && artifact.reference == reference)
            {
                ticket.artifacts.push(Artifact {
                    kind: kind.clone(),
                    reference,
                });
                ticket
                    .artifacts
                    .sort_by(|a, b| a.reference.cmp(&b.reference));
            }
            ticket.touch(&event.ts);
            Ok(())
        }
        "artifact_removed" => {
            let kind = ArtifactType::parse(extract_str(&event.data, "type")?)?;
            let reference = extract_str(&event.data, "ref")?;
            ticket
                .artifacts
                .retain(|artifact| !(artifact.kind == kind && artifact.reference == reference));
            ticket.touch(&event.ts);
            Ok(())
        }
        "assignees_added" => {
            let added = extract_string_list(&event.data, "added")?;
            for assignee in added {
                if !ticket.assignees.contains(&assignee) {
                    ticket.assignees.push(assignee);
                }
            }
            ticket.assignees.sort();
            ticket.assignees.dedup();
            ticket.touch(&event.ts);
            Ok(())
        }
        "assignees_removed" => {
            let removed = extract_string_list(&event.data, "removed")?;
            ticket
                .assignees
                .retain(|assignee| !removed.contains(assignee));
            ticket.touch(&event.ts);
            Ok(())
        }
        "assignees_set" => {
            let to = extract_string_list(&event.data, "to")?;
            ticket.assignees = to;
            ticket.touch(&event.ts);
            Ok(())
        }
        "tags_added" => {
            let added = extract_string_list(&event.data, "added")?;
            for tag in added {
                if !ticket.tags.contains(&tag) {
                    ticket.tags.push(tag);
                }
            }
            ticket.tags.sort();
            ticket.tags.dedup();
            ticket.touch(&event.ts);
            Ok(())
        }
        "tags_removed" => {
            let removed = extract_string_list(&event.data, "removed")?;
            ticket.tags.retain(|tag| !removed.contains(tag));
            ticket.touch(&event.ts);
            Ok(())
        }
        "tags_set" => {
            let to = extract_string_list(&event.data, "to")?;
            ticket.tags = to;
            ticket.touch(&event.ts);
            Ok(())
        }
        other => Err(TikError::Schema(format!("unsupported event kind: {other}"))),
    }
}

fn normalize_values(values: Vec<String>, label: &str, allow_empty: bool) -> Result<Vec<String>> {
    if values.is_empty() {
        if allow_empty {
            return Ok(Vec::new());
        }
        let msg = format!("{label} list cannot be empty");
        return Err(TikError::usage(&msg));
    }

    let mut out: Vec<String> = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    if out.is_empty() {
        let msg = format!("{label} list cannot be empty");
        return Err(TikError::usage(&msg));
    }
    out.sort();
    out.dedup();
    Ok(out)
}

#[cfg(test)]
mod notes_md_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn append_notes_md_writes_line() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("notes.md");
        append_notes_md(&path, "2026-01-01T00:00:00Z", "human", "note").unwrap();
        let contents = tikfs::read_to_string(&path).unwrap();
        assert!(contents.contains("human"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::milestone::NewMilestone;
    use crate::repo::Repo;
    use std::path::PathBuf;
    use tempfile::{tempdir, TempDir};

    struct TestRepo {
        _dir: TempDir,
        repo: Repo,
    }

    fn init_repo() -> TestRepo {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        TestRepo { _dir: dir, repo }
    }

    #[test]
    fn create_and_load_ticket() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;

        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Test".to_string(),
                    summary: None,
                    description: None,
                    tags: vec!["mvp".to_string()],
                },
                "human",
            )
            .unwrap();

        let loaded = repo.load_ticket(&ticket.id).unwrap();
        assert_eq!(loaded.title, "Test");
        assert_eq!(loaded.tags, vec!["mvp"]);

        let status = repo.status().unwrap();
        let notes_md = PathBuf::from(status.project_root)
            .join("tickets")
            .join(ticket.id.as_str())
            .join("notes.md");
        assert!(notes_md.is_file());
    }

    #[test]
    fn list_filters_by_status() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;

        repo.create_ticket(
            NewTicket {
                title: "Ticket A".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "human",
        )
        .unwrap();

        let tickets = repo.list_tickets(Some(TicketStatus::Open)).unwrap();
        assert_eq!(tickets.len(), 1);
    }

    #[test]
    fn load_missing_ticket_returns_error() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let missing = TicketId::new();
        let err = repo.load_ticket(&missing).unwrap_err();
        assert!(matches!(err, TikError::NotFound(_)));
    }

    #[test]
    fn append_note_updates_ticket() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let event = repo.append_note(&ticket.id, "human", "note").unwrap();
        assert_eq!(event.kind, "note");

        let events = repo.read_events(&ticket.id).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn read_ticket_notes_recreates_missing_file() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let status = repo.status().unwrap();
        let notes_path = PathBuf::from(status.project_root)
            .join("tickets")
            .join(ticket.id.as_str())
            .join("notes.md");
        std::fs::remove_file(&notes_path).unwrap();

        let contents = repo.read_ticket_notes(&ticket.id).unwrap();
        assert!(notes_path.is_file());
        assert!(contents.contains(ticket.id.as_str()));
    }

    #[test]
    fn read_notes_md_if_exists_returns_empty_when_missing() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let status = repo.status().unwrap();
        let data_root = PathBuf::from(&status.project_root);
        let notes_path = data_root
            .join("tickets")
            .join(ticket.id.as_str())
            .join("notes.md");
        std::fs::remove_file(&notes_path).unwrap();

        let schema_dir = PathBuf::from(&status.tik_root).join("schema");
        let store = TicketStore::new(data_root, schema_dir);
        let contents = store.read_notes_md_if_exists(&ticket.id).unwrap();
        assert_eq!(contents, "");
    }

    #[test]
    fn write_ticket_notes_updates_contents() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        repo.write_ticket_notes(&ticket.id, "custom notes\n")
            .unwrap();
        let contents = repo.read_ticket_notes(&ticket.id).unwrap();
        assert_eq!(contents, "custom notes\n");
    }

    #[test]
    fn set_milestone_updates_ticket() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let milestone = repo
            .create_milestone(
                NewMilestone {
                    title: "Phase 1".to_string(),
                    description: None,
                    due_at: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let updated = repo
            .set_ticket_milestone(&ticket.id, Some(&milestone.id), "human", None)
            .unwrap();
        assert_eq!(updated.milestone_id.as_ref(), Some(&milestone.id));

        let cleared = repo
            .set_ticket_milestone(&ticket.id, None, "human", Some("reset"))
            .unwrap();
        assert!(cleared.milestone_id.is_none());
    }

    #[test]
    fn set_milestone_requires_existing_milestone() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();
        let missing = MilestoneId::new();

        let err = repo
            .set_ticket_milestone(&ticket.id, Some(&missing), "human", None)
            .unwrap_err();
        assert!(matches!(err, TikError::NotFound(_)));
    }

    #[test]
    fn update_status_closes_and_reopens_ticket() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let closed = repo
            .update_status(&ticket.id, TicketStatus::Closed, "human", Some("done"))
            .unwrap();
        assert_eq!(closed.status, TicketStatus::Closed);
        assert!(closed.closed_at.is_some());

        let reopened = repo
            .update_status(&ticket.id, TicketStatus::Open, "human", None)
            .unwrap();
        assert_eq!(reopened.status, TicketStatus::Open);
        assert!(reopened.closed_at.is_none());

        let events = repo.read_events(&ticket.id).unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn update_status_missing_ticket_returns_error() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let missing = TicketId::new();
        let err = repo
            .update_status(&missing, TicketStatus::Closed, "human", None)
            .unwrap_err();
        assert!(matches!(err, TikError::NotFound(_)));
    }

    #[test]
    fn rebuild_from_events_applies_milestone() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();
        let milestone = repo
            .create_milestone(
                NewMilestone {
                    title: "Phase 1".to_string(),
                    description: None,
                    due_at: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        repo.set_ticket_milestone(&ticket.id, Some(&milestone.id), "human", None)
            .unwrap();
        let events = repo.read_events(&ticket.id).unwrap();

        let status = repo.status().unwrap();
        let schema_dir = PathBuf::from(&status.tik_root).join("schema");
        let store = TicketStore::new(PathBuf::from(status.project_root), schema_dir);
        let rebuilt = store.rebuild_from_events(&ticket.id, &events).unwrap();
        let current = repo.load_ticket(&ticket.id).unwrap();
        assert_eq!(rebuilt.milestone_id, current.milestone_id);
        assert_eq!(rebuilt.status, current.status);
    }

    #[test]
    fn import_ticket_inserts_snapshot_event() {
        let test_repo = init_repo();
        let status = test_repo.repo.status().unwrap();
        let data_root = PathBuf::from(status.project_root);
        let schema_dir = PathBuf::from(status.tik_root).join("schema");
        let store = TicketStore::new(data_root.clone(), schema_dir);
        let ticket = Ticket::new(
            NewTicket {
                title: "Imported".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );

        store
            .import_ticket(ticket.clone(), Vec::new(), None, "human")
            .unwrap();

        let events = store.read_events(&ticket.id).unwrap();
        assert_eq!(events[0].kind, "imported");
        assert!(events[0].data.get("ticket").is_some());
    }

    #[test]
    fn add_and_remove_relation() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let source = repo
            .create_ticket(
                NewTicket {
                    title: "Source".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();
        let target = repo
            .create_ticket(
                NewTicket {
                    title: "Target".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let updated = repo
            .add_relation(&source.id, RelationType::Blocks, &target.id, "human", None)
            .unwrap();
        assert_eq!(updated.relations.len(), 1);

        let events = repo.read_events(&source.id).unwrap();
        assert_eq!(events.len(), 2);

        let updated = repo
            .remove_relation(
                &source.id,
                RelationType::Blocks,
                &target.id,
                "human",
                Some("no longer needed"),
            )
            .unwrap();
        assert!(updated.relations.is_empty());

        let events = repo.read_events(&source.id).unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn add_relation_rejects_self() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let err = repo
            .add_relation(&ticket.id, RelationType::Blocks, &ticket.id, "human", None)
            .unwrap_err();
        assert!(matches!(err, TikError::Usage(_)));
    }

    #[test]
    fn add_relation_requires_target_exists() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();
        let missing = TicketId::new();

        let err = repo
            .add_relation(&ticket.id, RelationType::Blocks, &missing, "human", None)
            .unwrap_err();
        assert!(matches!(err, TikError::NotFound(_)));
    }

    #[test]
    fn add_and_remove_artifact() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let updated = repo
            .add_artifact(
                &ticket.id,
                ArtifactType::File,
                "docs/design.md",
                "human",
                None,
            )
            .unwrap();
        assert_eq!(updated.artifacts.len(), 1);

        let events = repo.read_events(&ticket.id).unwrap();
        assert_eq!(events.len(), 2);

        let updated = repo
            .remove_artifact(
                &ticket.id,
                ArtifactType::File,
                "docs/design.md",
                "human",
                Some("moved"),
            )
            .unwrap();
        assert!(updated.artifacts.is_empty());

        let events = repo.read_events(&ticket.id).unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn add_artifact_rejects_invalid_path() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let err = repo
            .add_artifact(
                &ticket.id,
                ArtifactType::File,
                "../secret.txt",
                "human",
                None,
            )
            .unwrap_err();
        assert!(matches!(err, TikError::Usage(_)));
    }

    #[test]
    fn add_remove_assignees() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let updated = repo
            .add_assignees(
                &ticket.id,
                vec!["alice".to_string(), "bob".to_string()],
                "human",
                None,
            )
            .unwrap();
        assert_eq!(updated.assignees, vec!["alice", "bob"]);

        let updated = repo
            .remove_assignees(&ticket.id, vec!["alice".to_string()], "human", None)
            .unwrap();
        assert_eq!(updated.assignees, vec!["bob"]);

        let events = repo.read_events(&ticket.id).unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn set_assignees_clears() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        repo.add_assignees(&ticket.id, vec!["alice".to_string()], "human", None)
            .unwrap();

        let updated = repo
            .set_assignees(&ticket.id, Vec::new(), "human", Some("reset"))
            .unwrap();
        assert!(updated.assignees.is_empty());
    }

    #[test]
    fn add_remove_tags() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let updated = repo
            .add_tags(
                &ticket.id,
                vec!["mvp".to_string(), "cli".to_string()],
                "human",
                None,
            )
            .unwrap();
        assert_eq!(updated.tags, vec!["cli", "mvp"]);

        let updated = repo
            .remove_tags(&ticket.id, vec!["cli".to_string()], "human", None)
            .unwrap();
        assert_eq!(updated.tags, vec!["mvp"]);

        let events = repo.read_events(&ticket.id).unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn set_tags_clears() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        repo.add_tags(&ticket.id, vec!["mvp".to_string()], "human", None)
            .unwrap();

        let updated = repo
            .set_tags(&ticket.id, Vec::new(), "human", Some("reset"))
            .unwrap();
        assert!(updated.tags.is_empty());
    }

    #[test]
    fn add_assignees_rejects_empty() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let err = repo
            .add_assignees(&ticket.id, vec!["   ".to_string()], "human", None)
            .unwrap_err();
        assert!(matches!(err, TikError::Usage(_)));
    }

    #[test]
    fn apply_edit_updates_ticket() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let mut edited = repo.load_ticket(&ticket.id).unwrap();
        edited.title = "Updated".to_string();
        let raw = serde_json::to_string_pretty(&edited).unwrap();

        let updated = repo
            .apply_edit(&ticket.id, &raw, "human", Some("manual"))
            .unwrap();
        assert_eq!(updated.title, "Updated");

        let events = repo.read_events(&ticket.id).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn apply_edit_rejects_id_mismatch() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Ticket".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let mut edited = repo.load_ticket(&ticket.id).unwrap();
        edited.id = TicketId::new();
        let raw = serde_json::to_string_pretty(&edited).unwrap();

        let err = repo
            .apply_edit(&ticket.id, &raw, "human", None)
            .unwrap_err();
        assert!(matches!(err, TikError::Schema(_)));
    }
}
