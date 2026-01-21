use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use std::time::Duration;

use crate::config::Config;
use crate::fs;
use crate::index::{IndexStore, IndexSummary};
use crate::interop::{ExportBundle, ExportMilestone, ExportTicket, ImportSummary};
use crate::lock::{self, LockGuard, LockKind};
use crate::report::{self, Graph, Report, Stats};
use crate::search::SearchQuery;
use crate::schema;
use crate::status::RepoStatus;
use crate::timeutil;
use crate::{
    domain::event::Event,
    domain::ids::{MilestoneId, TicketId},
    domain::milestone::{Milestone, MilestoneStatus, NewMilestone},
    domain::ticket::{NewTicket, Ticket, TicketStatus},
    store::milestone_store::MilestoneStore,
    store::ticket_store::TicketStore,
};
use crate::{Result, TikError};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RepoMeta {
    schema_version: String,
    layout_version: String,
    id_strategy: String,
    created_at: String,
    cli_version: String,
}

impl RepoMeta {
    fn new(cli_version: &str) -> Result<Self> {
        Ok(Self {
            schema_version: "1.0".to_string(),
            layout_version: "1.0".to_string(),
            id_strategy: "ulid".to_string(),
            created_at: timeutil::now_rfc3339()?,
            cli_version: cli_version.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Repo {
    root: PathBuf,
    tik_root: PathBuf,
    meta: RepoMeta,
    config: Config,
}

impl Repo {
    pub fn init(root: &Path, cli_version: &str) -> Result<Repo> {
        let root = root.to_path_buf();
        let tik_root = root.join(".tik");

        if tik_root.exists() {
            return Err(TikError::usage(".tik already exists"));
        }

        fs::ensure_dir(&tik_root)?;
        fs::ensure_dir(&tik_root.join("schema"))?;
        fs::ensure_dir(&tik_root.join("tickets"))?;
        fs::ensure_dir(&tik_root.join("milestones"))?;
        fs::ensure_dir(&tik_root.join("index"))?;
        fs::ensure_dir(&tik_root.join("locks"))?;
        fs::ensure_dir(&tik_root.join("tmp"))?;

        let meta = RepoMeta::new(cli_version)?;
        let config = Config::default();

        let repo_json = serde_json::to_string_pretty(&meta)
            .map_err(|err| TikError::Internal(format!("serialize repo.json: {err}")))?;
        fs::write_string_atomic(&tik_root.join("repo.json"), &repo_json)?;
        Config::write(&tik_root.join("config.json"), &config)?;
        schema::write_default_schemas(&tik_root.join("schema"))?;

        Ok(Repo {
            root,
            tik_root,
            meta,
            config,
        })
    }

    pub fn discover(start: &Path) -> Result<Repo> {
        let mut cursor = if start.is_file() {
            start
                .parent()
                .ok_or_else(|| TikError::repo_invalid("invalid start path"))?
                .to_path_buf()
        } else {
            start.to_path_buf()
        };

        loop {
            let tik_root = cursor.join(".tik");
            if tik_root.is_dir() {
                return Repo::open(&cursor);
            }

            if !cursor.pop() {
                break;
            }
        }

        Err(TikError::repo_invalid(".tik not found"))
    }

    pub fn open(root: &Path) -> Result<Repo> {
        let root = root.to_path_buf();
        let tik_root = root.join(".tik");
        if !tik_root.is_dir() {
            return Err(TikError::repo_invalid(".tik missing"));
        }

        let meta_path = tik_root.join("repo.json");
        let config_path = tik_root.join("config.json");
        let legacy_config_path = tik_root.join("config.toml");
        let schema_dir = tik_root.join("schema");

        if !meta_path.is_file() {
            return Err(TikError::repo_invalid("repo.json missing"));
        }
        schema::ensure_default_schemas(&schema_dir)?;

        let meta_raw = fs::read_to_string(&meta_path)?;
        let meta: RepoMeta = serde_json::from_str(&meta_raw)
            .map_err(|err| TikError::RepoInvalid(format!("invalid repo.json: {err}")))?;

        let config = if config_path.is_file() {
            Config::load(&config_path)?
        } else if legacy_config_path.is_file() {
            let config = Config::load_legacy_toml(&legacy_config_path)?;
            Config::write(&config_path, &config)?;
            config
        } else {
            return Err(TikError::repo_invalid("config.json missing"));
        };
        let registry = schema::SchemaRegistry::load(&schema_dir)?;
        registry.validate_config(&config)?;

        Ok(Repo {
            root,
            tik_root,
            meta,
            config,
        })
    }

    pub fn status(&self) -> Result<RepoStatus> {
        let _repo_lock = self.lock_repo_shared()?;
        let tickets_dir = self.tik_root.join("tickets");
        let milestones_dir = self.tik_root.join("milestones");
        let index_dir = self.tik_root.join("index");

        let ticket_count = count_dirs(&tickets_dir)?;
        let milestone_count = count_files_with_extension(&milestones_dir, "json")?;
        let index_present =
            index_dir.join("fts.sqlite").is_file() || index_dir.join("tickets.jsonl").is_file();

        Ok(RepoStatus {
            repo_root: self.root.display().to_string(),
            tik_root: self.tik_root.display().to_string(),
            schema_version: self.meta.schema_version.clone(),
            config_version: self.config.schema_version.clone(),
            layout_version: self.meta.layout_version.clone(),
            index_present,
            ticket_count,
            milestone_count,
        })
    }

    pub fn config_show(&self) -> Result<Config> {
        let _repo_lock = self.lock_repo_shared()?;
        Ok(self.config.clone())
    }

    pub fn config_get(&self, key: &str) -> Result<String> {
        let _repo_lock = self.lock_repo_shared()?;
        self.config.get_value(key)
    }

    pub fn config_set(&mut self, key: &str, value: &str) -> Result<Config> {
        let _repo_lock = self.lock_repo()?;
        let mut config = self.config.clone();
        config.set_value(key, value)?;
        let registry = schema::SchemaRegistry::load(&self.tik_root.join("schema"))?;
        registry.validate_config(&config)?;
        Config::write(&self.tik_root.join("config.json"), &config)?;
        self.config = config.clone();
        Ok(config)
    }

    pub fn create_ticket(&self, new_ticket: NewTicket, actor: &str) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        self.ticket_store().create(new_ticket, actor)
    }

    pub fn load_ticket(&self, id: &TicketId) -> Result<Ticket> {
        let _repo_lock = self.lock_repo_shared()?;
        self.ticket_store().load(id)
    }

    pub fn read_ticket_raw(&self, id: &TicketId) -> Result<String> {
        let _repo_lock = self.lock_repo_shared()?;
        self.ticket_store().read_raw(id)
    }

    pub fn read_ticket_notes(&self, id: &TicketId) -> Result<String> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store().read_notes_md(id)
    }

    pub fn write_ticket_notes(&self, id: &TicketId, contents: &str) -> Result<()> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store().write_notes_md(id, contents)
    }

    pub fn list_tickets(&self, status: Option<TicketStatus>) -> Result<Vec<Ticket>> {
        self.list_tickets_page(status, 0, None)
    }

    pub fn list_tickets_page(
        &self,
        status: Option<TicketStatus>,
        offset: usize,
        limit: Option<usize>,
    ) -> Result<Vec<Ticket>> {
        let _repo_lock = self.lock_repo_shared()?;

        if let Ok(ids) = self
            .index_store()
            .list_ids(status.as_ref().map(|value| value.as_str()), offset, limit)
        {
            let mut tickets = Vec::new();
            for id in ids {
                let ticket_id = match TicketId::parse(&id) {
                    Ok(ticket_id) => ticket_id,
                    Err(_) => {
                        return self
                            .ticket_store()
                            .list_page(status.clone(), offset, limit)
                    }
                };
                let ticket = match self.ticket_store().load(&ticket_id) {
                    Ok(ticket) => ticket,
                    Err(_) => {
                        return self
                            .ticket_store()
                            .list_page(status.clone(), offset, limit)
                    }
                };
                if let Some(ref filter) = status {
                    if &ticket.status != filter {
                        return self
                            .ticket_store()
                            .list_page(status.clone(), offset, limit);
                    }
                }
                tickets.push(ticket);
            }
            return Ok(tickets);
        }

        self.ticket_store().list_page(status, offset, limit)
    }

    pub fn create_milestone(&self, new_milestone: NewMilestone, actor: &str) -> Result<Milestone> {
        let _repo_lock = self.lock_repo()?;
        self.milestone_store().create(new_milestone, actor)
    }

    pub fn load_milestone(&self, id: &MilestoneId) -> Result<Milestone> {
        let _repo_lock = self.lock_repo_shared()?;
        self.milestone_store().load(id)
    }

    pub fn list_milestones(&self, status: Option<MilestoneStatus>) -> Result<Vec<Milestone>> {
        let _repo_lock = self.lock_repo_shared()?;
        self.milestone_store().list(status)
    }

    pub fn close_milestone(
        &self,
        id: &MilestoneId,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Milestone> {
        let _repo_lock = self.lock_repo()?;
        self.milestone_store().close(id, actor, reason)
    }

    pub fn read_milestone_events(&self, id: &MilestoneId) -> Result<Vec<Event>> {
        let _repo_lock = self.lock_repo_shared()?;
        self.milestone_store().read_events(id)
    }

    pub fn set_ticket_milestone(
        &self,
        id: &TicketId,
        milestone_id: Option<&MilestoneId>,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store()
            .set_milestone(id, milestone_id, actor, reason)
    }

    pub fn rebuild_index(&self) -> Result<IndexSummary> {
        let _repo_lock = self.lock_repo()?;
        let tickets = self.ticket_store().list(None)?;
        let mut notes = HashMap::new();
        for ticket in &tickets {
            let text = self.ticket_store().read_notes_md_if_exists(&ticket.id)?;
            notes.insert(ticket.id.as_str().to_string(), text);
        }
        self.index_store().rebuild(&tickets, &notes)
    }

    pub fn search(&self, query: &SearchQuery) -> Result<Vec<Ticket>> {
        self.search_page(query, 0, None)
    }

    pub fn search_page(
        &self,
        query: &SearchQuery,
        offset: usize,
        limit: Option<usize>,
    ) -> Result<Vec<Ticket>> {
        let _repo_lock = self.lock_repo_shared()?;
        let index = self.index_store();
        let ids = match index.search_page(query, offset, limit) {
            Ok(ids) => ids,
            Err(TikError::Index(_)) => {
                return search_fallback(self, query, offset, limit);
            }
            Err(err) => return Err(err),
        };

        let mut tickets = Vec::new();
        for id in ids {
            let ticket_id = match TicketId::parse(&id) {
                Ok(ticket_id) => ticket_id,
                Err(_) => return search_fallback(self, query, offset, limit),
            };
            let ticket = match self.ticket_store().load(&ticket_id) {
                Ok(ticket) => ticket,
                Err(_) => return search_fallback(self, query, offset, limit),
            };
            tickets.push(ticket);
        }
        Ok(tickets)
    }

    pub fn stats(&self) -> Result<Stats> {
        let _repo_lock = self.lock_repo_shared()?;
        let tickets = self.ticket_store().list(None)?;
        let milestones = self.milestone_store().list(None)?;
        Ok(report::compute_stats(&tickets, &milestones))
    }

    pub fn report(&self, recent_limit: usize) -> Result<Report> {
        let _repo_lock = self.lock_repo_shared()?;
        let tickets = self.ticket_store().list(None)?;
        let milestones = self.milestone_store().list(None)?;
        report::compute_report(&tickets, &milestones, recent_limit)
    }

    pub fn graph(&self) -> Result<Graph> {
        let _repo_lock = self.lock_repo_shared()?;
        let tickets = self.ticket_store().list(None)?;
        Ok(report::compute_graph(&tickets))
    }

    pub fn export_bundle(&self) -> Result<ExportBundle> {
        let _repo_lock = self.lock_repo_shared()?;
        let tickets = self.ticket_store().list(None)?;
        let mut export_tickets = Vec::new();
        for ticket in tickets {
            let events = self.ticket_store().read_events(&ticket.id)?;
            let notes_md = self.ticket_store().read_notes_md_if_exists(&ticket.id)?;
            export_tickets.push(ExportTicket {
                ticket,
                events,
                notes_md,
            });
        }

        let milestones = self.milestone_store().list(None)?;
        let mut export_milestones = Vec::new();
        for milestone in milestones {
            let events = self.milestone_store().read_events(&milestone.id)?;
            export_milestones.push(ExportMilestone { milestone, events });
        }

        Ok(ExportBundle {
            schema_version: "1.0".to_string(),
            exported_at: timeutil::now_rfc3339()?,
            tickets: export_tickets,
            milestones: export_milestones,
        })
    }

    pub fn import_bundle(&self, bundle: ExportBundle, actor: &str) -> Result<ImportSummary> {
        if bundle.schema_version != "1.0" {
            return Err(TikError::Schema(format!(
                "unsupported export schema_version {}",
                bundle.schema_version
            )));
        }

        let _repo_lock = self.lock_repo()?;
        let mut summary = ImportSummary {
            tickets_imported: 0,
            milestones_imported: 0,
        };

        for milestone in bundle.milestones {
            self.milestone_store().import_milestone(
                milestone.milestone,
                milestone.events,
                actor,
            )?;
            summary.milestones_imported += 1;
        }

        for ticket in bundle.tickets {
            let ExportTicket {
                ticket,
                events,
                notes_md,
            } = ticket;
            let notes_md = if notes_md.trim().is_empty() {
                None
            } else {
                Some(notes_md)
            };
            self.ticket_store()
                .import_ticket(ticket, events, notes_md, actor)?;
            summary.tickets_imported += 1;
        }

        Ok(summary)
    }

    pub fn append_note(&self, id: &TicketId, actor: &str, text: &str) -> Result<Event> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store().append_note(id, actor, text)
    }

    pub fn update_status(
        &self,
        id: &TicketId,
        status: TicketStatus,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store().update_status(id, status, actor, reason)
    }

    pub fn add_relation(
        &self,
        id: &TicketId,
        kind: crate::domain::ticket::RelationType,
        target: &TicketId,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store()
            .add_relation(id, kind, target, actor, reason)
    }

    pub fn remove_relation(
        &self,
        id: &TicketId,
        kind: crate::domain::ticket::RelationType,
        target: &TicketId,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store()
            .remove_relation(id, kind, target, actor, reason)
    }

    pub fn add_artifact(
        &self,
        id: &TicketId,
        kind: crate::domain::ticket::ArtifactType,
        reference: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store()
            .add_artifact(id, kind, reference, actor, reason)
    }

    pub fn remove_artifact(
        &self,
        id: &TicketId,
        kind: crate::domain::ticket::ArtifactType,
        reference: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store()
            .remove_artifact(id, kind, reference, actor, reason)
    }

    pub fn add_assignees(
        &self,
        id: &TicketId,
        assignees: Vec<String>,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store()
            .add_assignees(id, assignees, actor, reason)
    }

    pub fn remove_assignees(
        &self,
        id: &TicketId,
        assignees: Vec<String>,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store()
            .remove_assignees(id, assignees, actor, reason)
    }

    pub fn set_assignees(
        &self,
        id: &TicketId,
        assignees: Vec<String>,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store()
            .set_assignees(id, assignees, actor, reason)
    }

    pub fn add_tags(
        &self,
        id: &TicketId,
        tags: Vec<String>,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store().add_tags(id, tags, actor, reason)
    }

    pub fn remove_tags(
        &self,
        id: &TicketId,
        tags: Vec<String>,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store().remove_tags(id, tags, actor, reason)
    }

    pub fn set_tags(
        &self,
        id: &TicketId,
        tags: Vec<String>,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store().set_tags(id, tags, actor, reason)
    }

    pub fn apply_edit(
        &self,
        id: &TicketId,
        raw_json: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store().apply_edit(id, raw_json, actor, reason)
    }

    pub fn touch_notes(&self, id: &TicketId, actor: &str, reason: Option<&str>) -> Result<Event> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        self.ticket_store().touch_notes(id, actor, reason)
    }

    pub fn read_events(&self, id: &TicketId) -> Result<Vec<Event>> {
        let _repo_lock = self.lock_repo_shared()?;
        self.ticket_store().read_events(id)
    }

    fn ticket_store(&self) -> TicketStore {
        TicketStore::new(self.tik_root.clone())
    }

    fn milestone_store(&self) -> MilestoneStore {
        MilestoneStore::new(self.tik_root.clone())
    }

    fn index_store(&self) -> IndexStore {
        IndexStore::new(self.tik_root.clone())
    }

    fn lock_repo(&self) -> Result<LockGuard> {
        fs::ensure_dir(&self.tik_root.join("locks"))?;
        let path = self.tik_root.join("locks").join("repo.lock");
        lock::acquire_lock(&path, LockKind::Exclusive, Duration::from_secs(2))
    }

    fn lock_repo_shared(&self) -> Result<LockGuard> {
        fs::ensure_dir(&self.tik_root.join("locks"))?;
        let path = self.tik_root.join("locks").join("repo.lock");
        lock::acquire_lock(&path, LockKind::Shared, Duration::from_secs(2))
    }

    fn lock_ticket(&self, id: &TicketId) -> Result<LockGuard> {
        fs::ensure_dir(&self.tik_root.join("locks"))?;
        let path = self
            .tik_root
            .join("locks")
            .join(format!("{}.lock", id.as_str()));
        lock::acquire_lock(&path, LockKind::Exclusive, Duration::from_secs(2))
    }
}

fn search_fallback(
    repo: &Repo,
    query: &SearchQuery,
    offset: usize,
    limit: Option<usize>,
) -> Result<Vec<Ticket>> {
    let tickets = repo.ticket_store().list(None)?;
    let mut matches = Vec::new();
    for ticket in tickets {
        let notes = repo.ticket_store().read_notes_md_if_exists(&ticket.id)?;
        if query.matches_ticket(&ticket, &notes) {
            matches.push(ticket);
        }
    }
    matches.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });
    let iter = matches.into_iter().skip(offset);
    let page = match limit {
        Some(limit) => iter.take(limit).collect(),
        None => iter.collect(),
    };
    Ok(page)
}

fn count_dirs(path: &Path) -> Result<u64> {
    if !path.exists() {
        return Ok(0);
    }

    let mut count = 0u64;
    for entry in std::fs::read_dir(path).map_err(|err| TikError::io("read dir", err))? {
        let entry = entry.map_err(|err| TikError::io("read dir entry", err))?;
        if entry
            .file_type()
            .map_err(|err| TikError::io("read file type", err))?
            .is_dir()
        {
            count += 1;
        }
    }
    Ok(count)
}

fn count_files_with_extension(path: &Path, extension: &str) -> Result<u64> {
    if !path.exists() {
        return Ok(0);
    }
    let mut count = 0u64;
    for entry in std::fs::read_dir(path).map_err(|err| TikError::io("read dir", err))? {
        let entry = entry.map_err(|err| TikError::io("read dir entry", err))?;
        if !entry
            .file_type()
            .map_err(|err| TikError::io("read file type", err))?
            .is_file()
        {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some(extension) {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ticket::RelationType;
    use crate::domain::ticket::NewTicket;
    use crate::search::SearchQuery;
    use tempfile::tempdir;

    #[test]
    fn init_creates_layout() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        let tik_root = dir.path().join(".tik");
        assert!(tik_root.is_dir());
        assert!(tik_root.join("schema").is_dir());
        assert!(tik_root.join("tickets").is_dir());
        assert!(tik_root.join("milestones").is_dir());
        assert!(tik_root.join("repo.json").is_file());
        assert!(tik_root.join("config.json").is_file());
        assert_eq!(repo.config.schema_version, "1.0");
    }

    #[test]
    fn init_rejects_existing_repo() {
        let dir = tempdir().unwrap();
        Repo::init(dir.path(), "0.1.0").unwrap();
        let err = Repo::init(dir.path(), "0.1.0").unwrap_err();
        assert!(matches!(err, TikError::Usage(_)));
    }

    #[test]
    fn discover_finds_repo_root() {
        let dir = tempdir().unwrap();
        Repo::init(dir.path(), "0.1.0").unwrap();
        let nested = dir.path().join("a/b/c");
        std::fs::create_dir_all(&nested).unwrap();
        let repo = Repo::discover(&nested).unwrap();
        assert_eq!(repo.root, dir.path());
    }

    #[test]
    fn discover_missing_repo_returns_error() {
        let dir = tempdir().unwrap();
        let err = Repo::discover(dir.path()).unwrap_err();
        assert!(matches!(err, TikError::RepoInvalid(_)));
    }

    #[test]
    fn status_counts_tickets() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        repo.create_ticket(
            NewTicket {
                title: "A".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "human",
        )
        .unwrap();
        repo.create_ticket(
            NewTicket {
                title: "B".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "human",
        )
        .unwrap();
        let status = repo.status().unwrap();
        assert_eq!(status.ticket_count, 2);
    }

    #[test]
    fn search_falls_back_without_index() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        repo.create_ticket(
            NewTicket {
                title: "Search me".to_string(),
                summary: None,
                description: None,
                tags: vec!["mvp".to_string()],
            },
            "human",
        )
        .unwrap();

        let query = SearchQuery::parse("status:open tag:mvp Search").unwrap();
        let results = repo.search(&query).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn list_tickets_page_paginates() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        let ticket_a = repo
            .create_ticket(
                NewTicket {
                    title: "A".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();
        let ticket_b = repo
            .create_ticket(
                NewTicket {
                    title: "B".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();

        let first = repo.list_tickets_page(None, 0, Some(1)).unwrap();
        let second = repo.list_tickets_page(None, 1, Some(1)).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_ne!(first[0].id, second[0].id);
        assert!(first[0].id == ticket_a.id || first[0].id == ticket_b.id);
    }

    #[test]
    fn search_page_respects_offset() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        repo.create_ticket(
            NewTicket {
                title: "Searchable 1".to_string(),
                summary: None,
                description: None,
                tags: vec!["mvp".to_string()],
            },
            "human",
        )
        .unwrap();
        repo.create_ticket(
            NewTicket {
                title: "Searchable 2".to_string(),
                summary: None,
                description: None,
                tags: vec!["mvp".to_string()],
            },
            "human",
        )
        .unwrap();

        let query = SearchQuery::parse("tag:mvp").unwrap();
        let page = repo.search_page(&query, 1, Some(1)).unwrap();
        assert_eq!(page.len(), 1);
    }

    #[test]
    fn rebuild_index_then_search() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        repo.create_ticket(
            NewTicket {
                title: "Indexed".to_string(),
                summary: None,
                description: None,
                tags: vec!["cli".to_string()],
            },
            "human",
        )
        .unwrap();

        let summary = repo.rebuild_index().unwrap();
        assert_eq!(summary.ticket_count, 1);

        let query = SearchQuery::parse("tag:cli Indexed").unwrap();
        let results = repo.search(&query).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn export_import_roundtrip() {
        let source_dir = tempdir().unwrap();
        let source = Repo::init(source_dir.path(), "0.1.0").unwrap();
        source
            .create_ticket(
                NewTicket {
                    title: "Exported".to_string(),
                    summary: None,
                    description: None,
                    tags: vec!["mvp".to_string()],
                },
                "human",
            )
            .unwrap();
        source
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

        let bundle = source.export_bundle().unwrap();

        let target_dir = tempdir().unwrap();
        let target = Repo::init(target_dir.path(), "0.1.0").unwrap();
        let summary = target.import_bundle(bundle, "human").unwrap();
        assert_eq!(summary.tickets_imported, 1);
        assert_eq!(summary.milestones_imported, 1);

        let status = target.status().unwrap();
        assert_eq!(status.ticket_count, 1);
        assert_eq!(status.milestone_count, 1);
    }

    #[test]
    fn stats_includes_milestones() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Tracked".to_string(),
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

        let stats = repo.stats().unwrap();
        assert_eq!(stats.tickets_total, 1);
        assert_eq!(stats.milestones_total, 1);
        assert_eq!(
            stats.tickets_by_milestone.get(milestone.id.as_str()),
            Some(&1)
        );
    }

    #[test]
    fn report_includes_recent_and_milestones() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Report".to_string(),
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

        let report = repo.report(5).unwrap();
        assert_eq!(report.recent_tickets.len(), 1);
        assert_eq!(report.milestones.len(), 1);
        let summary = &report.milestones[0];
        assert_eq!(summary.total_tickets, 1);
        assert_eq!(summary.open_tickets, 1);
    }

    #[test]
    fn graph_includes_relations() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        let ticket_a = repo
            .create_ticket(
                NewTicket {
                    title: "Graph A".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();
        let ticket_b = repo
            .create_ticket(
                NewTicket {
                    title: "Graph B".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "human",
            )
            .unwrap();
        repo.add_relation(
            &ticket_a.id,
            RelationType::Blocks,
            &ticket_b.id,
            "human",
            None,
        )
        .unwrap();

        let graph = repo.graph().unwrap();
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
    }
}
