use std::fs::OpenOptions;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use crate::domain::event::Event;
use crate::fs;
use crate::{Result, TikError};

#[derive(Debug, Clone)]
pub struct EventStore {
    path: PathBuf,
}

impl EventStore {
    pub fn new(path: PathBuf) -> EventStore {
        EventStore { path }
    }

    pub fn append(&self, event: &Event) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::ensure_dir(parent)?;
        }

        let line = serde_json::to_string(event)
            .map_err(|err| TikError::Schema(format!("serialize event: {err}")))?;
        let line = format!("{line}\n");
        fs::append_string_atomic(&self.path, &line)?;
        Ok(())
    }

    pub fn read_all(&self) -> Result<Vec<Event>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let file = OpenOptions::new()
            .read(true)
            .open(&self.path)
            .map_err(|err| TikError::io("open notes log", err))?;
        let reader = BufReader::new(file);

        let mut events = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(|err| TikError::io("read notes log", err))?;
            if line.trim().is_empty() {
                continue;
            }
            let event: Event = serde_json::from_str(&line)
                .map_err(|err| TikError::Schema(format!("invalid event json: {err}")))?;
            events.push(event);
        }

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::event::Event;
    use tempfile::tempdir;

    #[test]
    fn append_and_read_events() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("notes.jsonl");
        let store = EventStore::new(path);

        let event1 = Event::note("human", "2026-01-01T00:00:00Z", "hello");
        let event2 = Event::note("agent", "2026-01-01T00:01:00Z", "next");
        store.append(&event1).unwrap();
        store.append(&event2).unwrap();

        let events = store.read_all().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].actor, "human");
        assert_eq!(events[1].actor, "agent");
    }

    #[test]
    fn read_all_handles_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.jsonl");
        let store = EventStore::new(path);

        let events = store.read_all().unwrap();
        assert!(events.is_empty());
    }
}
