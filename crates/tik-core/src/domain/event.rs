use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::domain::ids::EventId;

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

    pub fn note(actor: &str, ts: &str, text: &str) -> Event {
        Event::new("note", actor, ts, json!({"text": text}))
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
