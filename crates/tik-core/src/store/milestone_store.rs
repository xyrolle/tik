use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::domain::event::Event;
use crate::domain::ids::MilestoneId;
use crate::domain::milestone::{Milestone, MilestoneStatus, NewMilestone};
use crate::fs as tikfs;
use crate::schema::SchemaRegistry;
use crate::store::event_store::EventStore;
use crate::timeutil;
use crate::{Result, TikError};

#[derive(Debug, Clone)]
pub struct MilestoneStore {
    tik_root: PathBuf,
}

impl MilestoneStore {
    pub fn new(tik_root: PathBuf) -> MilestoneStore {
        MilestoneStore { tik_root }
    }

    pub fn create(&self, new_milestone: NewMilestone, actor: &str) -> Result<Milestone> {
        let now = timeutil::now_rfc3339()?;
        let mut milestone = Milestone::new(new_milestone, &now);
        milestone.tags = normalize_values(milestone.tags, "tag", true)?;

        let path = self.milestone_path(&milestone.id);
        if path.exists() {
            return Err(TikError::Conflict("milestone file exists".to_string()));
        }

        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_milestone(&milestone)?;
        tikfs::ensure_dir(&self.milestones_dir())?;

        let event = Event::new(
            "milestone_created",
            actor,
            &now,
            json!({"title": milestone.title, "status": milestone.status.as_str(), "tags": milestone.tags}),
        );
        schemas.validate_event(&event)?;
        EventStore::new(self.milestone_log_path(&milestone.id)).append(&event)?;

        let milestone_json = serde_json::to_string_pretty(&milestone)
            .map_err(|err| TikError::Schema(format!("serialize milestone: {err}")))?;
        tikfs::write_string_atomic(&path, &milestone_json)?;

        Ok(milestone)
    }

    pub fn import_milestone(
        &self,
        milestone: Milestone,
        events: Vec<Event>,
        actor: &str,
    ) -> Result<Milestone> {
        let path = self.milestone_path(&milestone.id);
        if path.exists() {
            return Err(TikError::Conflict("milestone file exists".to_string()));
        }

        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_milestone(&milestone)?;
        tikfs::ensure_dir(&self.milestones_dir())?;

        let events = if events.is_empty() {
            let now = timeutil::now_rfc3339()?;
            vec![Event::new("milestone_imported", actor, &now, json!({}))]
        } else {
            events
        };
        write_events_jsonl(&self.milestone_log_path(&milestone.id), &events, &schemas)?;

        let milestone_json = serde_json::to_string_pretty(&milestone)
            .map_err(|err| TikError::Schema(format!("serialize milestone: {err}")))?;
        tikfs::write_string_atomic(&path, &milestone_json)?;

        Ok(milestone)
    }

    pub fn load(&self, id: &MilestoneId) -> Result<Milestone> {
        let path = self.milestone_path(id);
        if !path.is_file() {
            return Err(TikError::NotFound(format!("milestone {}", id.as_str())));
        }

        let raw = tikfs::read_to_string(&path)?;
        let milestone: Milestone = serde_json::from_str(&raw)
            .map_err(|err| TikError::Schema(format!("invalid milestone json: {err}")))?;
        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_milestone(&milestone)?;
        Ok(milestone)
    }

    pub fn list(&self, status: Option<MilestoneStatus>) -> Result<Vec<Milestone>> {
        let mut milestones = Vec::new();
        let milestones_dir = self.milestones_dir();
        if !milestones_dir.exists() {
            return Ok(milestones);
        }
        let schemas = SchemaRegistry::load(&self.schema_dir())?;

        for entry in
            fs::read_dir(&milestones_dir).map_err(|err| TikError::io("read milestones dir", err))?
        {
            let entry = entry.map_err(|err| TikError::io("read milestones dir", err))?;
            if !entry
                .file_type()
                .map_err(|err| TikError::io("read dir entry", err))?
                .is_file()
            {
                continue;
            }

            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }

            let raw = tikfs::read_to_string(&path)?;
            let milestone: Milestone = serde_json::from_str(&raw)
                .map_err(|err| TikError::Schema(format!("invalid milestone json: {err}")))?;
            schemas.validate_milestone(&milestone)?;
            if let Some(ref filter) = status {
                if &milestone.status != filter {
                    continue;
                }
            }
            milestones.push(milestone);
        }

        milestones.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        Ok(milestones)
    }

    pub fn close(&self, id: &MilestoneId, actor: &str, reason: Option<&str>) -> Result<Milestone> {
        let mut milestone = self.load(id)?;
        let now = timeutil::now_rfc3339()?;
        milestone.set_status(MilestoneStatus::Closed, &now);

        let schemas = SchemaRegistry::load(&self.schema_dir())?;
        schemas.validate_milestone(&milestone)?;

        let data = if let Some(reason) = reason {
            json!({"reason": reason})
        } else {
            json!({})
        };
        let event = Event::new("milestone_closed", actor, &now, data);
        schemas.validate_event(&event)?;
        EventStore::new(self.milestone_log_path(id)).append(&event)?;

        let milestone_json = serde_json::to_string_pretty(&milestone)
            .map_err(|err| TikError::Schema(format!("serialize milestone: {err}")))?;
        tikfs::write_string_atomic(&self.milestone_path(id), &milestone_json)?;

        Ok(milestone)
    }

    pub fn read_events(&self, id: &MilestoneId) -> Result<Vec<Event>> {
        let path = self.milestone_log_path(id);
        EventStore::new(path).read_all()
    }

    fn milestones_dir(&self) -> PathBuf {
        self.tik_root.join("milestones")
    }

    fn milestone_path(&self, id: &MilestoneId) -> PathBuf {
        self.milestones_dir().join(format!("{}.json", id.as_str()))
    }

    fn milestone_log_path(&self, id: &MilestoneId) -> PathBuf {
        self.milestones_dir().join(format!("{}.jsonl", id.as_str()))
    }

    fn schema_dir(&self) -> PathBuf {
        self.tik_root.join("schema")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::Repo;
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
    fn create_and_load_milestone() {
        let test_repo = init_repo();
        let repo = &test_repo.repo;
        let milestone = repo
            .create_milestone(
                NewMilestone {
                    title: "Phase 1".to_string(),
                    description: None,
                    due_at: None,
                    tags: vec!["mvp".to_string()],
                },
                "human",
            )
            .unwrap();

        let loaded = repo.load_milestone(&milestone.id).unwrap();
        assert_eq!(loaded.title, "Phase 1");
        assert_eq!(loaded.tags, vec!["mvp"]);
    }

    #[test]
    fn list_filters_by_status() {
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

        repo.close_milestone(&milestone.id, "human", None).unwrap();

        let open = repo.list_milestones(Some(MilestoneStatus::Open)).unwrap();
        let closed = repo
            .list_milestones(Some(MilestoneStatus::Closed))
            .unwrap();
        assert!(open.is_empty());
        assert_eq!(closed.len(), 1);
    }

    #[test]
    fn close_updates_status() {
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

        let closed = repo
            .close_milestone(&milestone.id, "human", Some("done"))
            .unwrap();
        assert_eq!(closed.status, MilestoneStatus::Closed);
    }
}
