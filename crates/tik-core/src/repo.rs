use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use std::time::Duration;

use crate::config::Config;
use crate::doctor::{DoctorCheck, DoctorReport, DoctorStatus};
use crate::fs;
use crate::index::{IndexStatus, IndexStore, IndexSummary};
use crate::interop::{ExportBundle, ExportMilestone, ExportTicket, ImportSummary};
use crate::lock::{self, LockGuard, LockKind};
use crate::migrate::{MigrationMove, MigrationSummary};
use crate::report::{
    self, BurndownReport, Graph, GraphOptions, Report, ReportFilters, ReportGroupBy, Stats,
    ThroughputReport, TicketHistory,
};
use crate::schema;
use crate::search::SearchQuery;
use crate::sort::TicketSort;
use crate::status::RepoStatus;
use crate::store::event_store::EventStore;
use crate::timeutil;
use crate::workspace::{ProjectMeta, WorkspaceConfig};
use crate::{
    domain::event::Event,
    domain::ids::{MilestoneId, TicketId},
    domain::milestone::{Milestone, MilestoneStatus, NewMilestone},
    domain::ticket::{NewTicket, Ticket, TicketStatus},
    store::milestone_store::MilestoneStore,
    store::ticket_store::TicketStore,
};
use crate::{Result, TikError};

/// Summary of a backup operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSummary {
    pub started_at: String,
    pub completed_at: String,
    pub output_path: String,
    pub tickets_count: usize,
    pub milestones_count: usize,
    pub project: String,
}

/// Summary of a restore operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreSummary {
    pub started_at: String,
    pub completed_at: String,
    pub input_path: String,
    pub tickets_count: usize,
    pub milestones_count: usize,
}

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
            layout_version: "2.0".to_string(),
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
    data_root: PathBuf,
    project: String,
    meta: RepoMeta,
    config: Config,
    config_path: PathBuf,
    workspace: Option<WorkspaceConfig>,
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
        fs::ensure_dir(&tik_root.join("projects"))?;
        fs::ensure_dir(&tik_root.join("locks"))?;
        fs::ensure_dir(&tik_root.join("tmp"))?;

        let meta = RepoMeta::new(cli_version)?;
        let config = Config::default();
        let schema_dir = tik_root.join("schema");

        let repo_json = serde_json::to_string_pretty(&meta)
            .map_err(|err| TikError::Internal(format!("serialize repo.json: {err}")))?;
        fs::write_string_atomic(&tik_root.join("repo.json"), &repo_json)?;
        Config::write(&tik_root.join("config.json"), &config)?;
        schema::write_default_schemas(&schema_dir)?;

        let workspace = WorkspaceConfig::new("default")?;
        let workspace_path = crate::workspace::workspace_path(&tik_root);
        WorkspaceConfig::write(&workspace_path, &workspace, &schema_dir)?;

        let project = ProjectMeta::new("default", None)?;
        let project_root = crate::workspace::project_root(&tik_root, &project.name)?;
        init_project_layout(&project_root, &project, &config, &schema_dir)?;

        Ok(Repo {
            root,
            tik_root,
            data_root: project_root.clone(),
            project: project.name.clone(),
            meta,
            config,
            config_path: project_root.join("config.json"),
            workspace: Some(workspace),
        })
    }

    pub fn discover(start: &Path) -> Result<Repo> {
        Repo::discover_with_project(start, None)
    }

    pub fn discover_with_project(start: &Path, project: Option<&str>) -> Result<Repo> {
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
                return Repo::open_with_project(&cursor, project);
            }

            if !cursor.pop() {
                break;
            }
        }

        Err(TikError::repo_invalid(".tik not found"))
    }

    pub fn open(root: &Path) -> Result<Repo> {
        Repo::open_with_project(root, None)
    }

    pub fn open_with_project(root: &Path, project: Option<&str>) -> Result<Repo> {
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

        let global_config = if config_path.is_file() {
            Config::load(&config_path)?
        } else if legacy_config_path.is_file() {
            let config = Config::load_legacy_toml(&legacy_config_path)?;
            Config::write(&config_path, &config)?;
            config
        } else {
            return Err(TikError::repo_invalid("config.json missing"));
        };
        let registry = schema::SchemaRegistry::load(&schema_dir)?;
        registry.validate_config(&global_config)?;

        if meta.layout_version == "1.0" {
            if let Some(project) = project {
                if project != "default" {
                    return Err(TikError::usage(
                        "projects require layout_version 2.0; run `tik migrate`",
                    ));
                }
            }
            return Ok(Repo {
                root,
                tik_root: tik_root.clone(),
                data_root: tik_root.clone(),
                project: "default".to_string(),
                meta,
                config: global_config.clone(),
                config_path,
                workspace: None,
            });
        }

        if meta.layout_version != "2.0" {
            return Err(TikError::RepoInvalid(format!(
                "unsupported layout_version {}",
                meta.layout_version
            )));
        }

        let workspace_path = crate::workspace::workspace_path(&tik_root);
        let workspace = WorkspaceConfig::load(&workspace_path, &schema_dir)?;
        let selected_project = match project {
            Some(project) => crate::workspace::validate_project_name(project)?,
            None => workspace.default_project.clone(),
        };
        let project_root = crate::workspace::project_root(&tik_root, &selected_project)?;
        if !project_root.is_dir() {
            return Err(TikError::NotFound(format!(
                "project {} not found",
                selected_project
            )));
        }

        let project_config_path = project_root.join("config.json");
        let config = if project_config_path.is_file() {
            let project_config = Config::load(&project_config_path)?;
            registry.validate_config(&project_config)?;
            project_config
        } else {
            global_config.clone()
        };

        Ok(Repo {
            root,
            tik_root,
            data_root: project_root.clone(),
            project: selected_project,
            meta,
            config,
            config_path: project_config_path,
            workspace: Some(workspace),
        })
    }

    pub fn status(&self) -> Result<RepoStatus> {
        let _repo_lock = self.lock_repo_shared()?;
        let tickets_dir = self.data_root.join("tickets");
        let milestones_dir = self.data_root.join("milestones");
        let index_dir = self.data_root.join("index");

        let ticket_count = count_dirs(&tickets_dir)?;
        let milestone_count = count_files_with_extension(&milestones_dir, "json")?;
        let index_present =
            index_dir.join("fts.sqlite").is_file() || index_dir.join("tickets.jsonl").is_file();

        Ok(RepoStatus {
            repo_root: self.root.display().to_string(),
            tik_root: self.tik_root.display().to_string(),
            project: self.project.clone(),
            project_root: self.data_root.display().to_string(),
            schema_version: self.meta.schema_version.clone(),
            config_version: self.config.schema_version.clone(),
            layout_version: self.meta.layout_version.clone(),
            index_present,
            ticket_count,
            milestone_count,
        })
    }

    pub fn ticket_path(&self, id: &TicketId) -> PathBuf {
        self.data_root
            .join("tickets")
            .join(id.as_str())
            .join("ticket.json")
    }

    pub fn notes_path(&self, id: &TicketId) -> PathBuf {
        self.data_root
            .join("tickets")
            .join(id.as_str())
            .join("notes.md")
    }

    pub fn project_list(&self) -> Result<Vec<ProjectMeta>> {
        self.ensure_projects_enabled()?;
        let _lock = self.lock_workspace(LockKind::Shared)?;
        let projects_dir = crate::workspace::projects_dir(&self.tik_root);
        if !projects_dir.exists() {
            return Ok(Vec::new());
        }
        let schema_dir = self.schema_dir();
        let mut projects = Vec::new();
        for entry in std::fs::read_dir(&projects_dir)
            .map_err(|err| TikError::io("read projects dir", err))?
        {
            let entry = entry.map_err(|err| TikError::io("read projects dir", err))?;
            if !entry
                .file_type()
                .map_err(|err| TikError::io("read project entry type", err))?
                .is_dir()
            {
                continue;
            }
            let project_root = entry.path();
            let meta_path = crate::workspace::project_meta_path(&project_root);
            if !meta_path.is_file() {
                return Err(TikError::RepoInvalid(format!(
                    "project metadata missing at {}",
                    meta_path.display()
                )));
            }
            let project = ProjectMeta::load(&meta_path, &schema_dir)?;
            projects.push(project);
        }
        projects.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(projects)
    }

    pub fn project_init(&mut self, name: &str, description: Option<&str>) -> Result<ProjectMeta> {
        self.ensure_projects_enabled()?;
        let _lock = self.lock_workspace(LockKind::Exclusive)?;
        let schema_dir = self.schema_dir();
        let project = ProjectMeta::new(name, description)?;
        let project_root = crate::workspace::project_root(&self.tik_root, &project.name)?;
        if project_root.exists() {
            return Err(TikError::Conflict(format!(
                "project {} already exists",
                project.name
            )));
        }
        let global_config = Config::load(&self.global_config_path())?;
        init_project_layout(&project_root, &project, &global_config, &schema_dir)?;
        Ok(project)
    }

    pub fn project_select(&mut self, name: &str) -> Result<ProjectMeta> {
        self.ensure_projects_enabled()?;
        let _lock = self.lock_workspace(LockKind::Exclusive)?;
        let schema_dir = self.schema_dir();
        let workspace_path = crate::workspace::workspace_path(&self.tik_root);
        let mut workspace = WorkspaceConfig::load(&workspace_path, &schema_dir)?;
        let selected = crate::workspace::validate_project_name(name)?;
        let project_root = crate::workspace::project_root(&self.tik_root, &selected)?;
        let project_path = crate::workspace::project_meta_path(&project_root);
        if !project_path.is_file() {
            return Err(TikError::NotFound(format!(
                "project {} not found",
                selected
            )));
        }
        let project = ProjectMeta::load(&project_path, &schema_dir)?;
        workspace.default_project = selected.clone();
        WorkspaceConfig::write(&workspace_path, &workspace, &schema_dir)?;

        self.workspace = Some(workspace);
        self.project = selected;
        self.data_root = project_root.clone();
        self.config_path = crate::workspace::project_config_path(&project_root);

        let registry = schema::SchemaRegistry::load(&schema_dir)?;
        let global_config = Config::load(&self.global_config_path())?;
        let config = if self.config_path.is_file() {
            let project_config = Config::load(&self.config_path)?;
            registry.validate_config(&project_config)?;
            project_config
        } else {
            global_config
        };
        self.config = config;

        Ok(project)
    }

    pub fn doctor(&self, all_projects: bool) -> Result<DoctorReport> {
        let mut checks = Vec::new();
        let repo_scope = "repo";
        let layout_version = self.meta.layout_version.clone();
        match layout_version.as_str() {
            "2.0" => checks.push(DoctorCheck::ok(
                "layout_version",
                repo_scope,
                "layout_version 2.0",
            )),
            "1.0" => checks.push(DoctorCheck::warn(
                "layout_version",
                repo_scope,
                "layout_version 1.0 (legacy)",
                Vec::new(),
                Some("run `tik migrate` to upgrade the workspace layout"),
            )),
            other => checks.push(DoctorCheck::error(
                "layout_version",
                repo_scope,
                "unsupported layout_version",
                vec![other.to_string()],
                Some("restore repo.json or upgrade with `tik migrate`"),
            )),
        }

        let schema_dir = self.schema_dir();
        let registry = match schema::SchemaRegistry::load(&schema_dir) {
            Ok(registry) => {
                checks.push(DoctorCheck::ok(
                    "schema_registry",
                    repo_scope,
                    "schema registry loaded",
                ));
                Some(registry)
            }
            Err(err) => {
                checks.push(DoctorCheck::error(
                    "schema_registry",
                    repo_scope,
                    "schema registry failed to load",
                    vec![err.to_string()],
                    Some("restore `.tik/schema` or reinitialize schema files"),
                ));
                None
            }
        };

        let global_config_path = self.global_config_path();
        match Config::load(&global_config_path) {
            Ok(config) => {
                if let Some(registry) = registry.as_ref() {
                    match registry.validate_config(&config) {
                        Ok(()) => checks.push(DoctorCheck::ok(
                            "config_global",
                            repo_scope,
                            "global config valid",
                        )),
                        Err(err) => checks.push(DoctorCheck::error(
                            "config_global",
                            repo_scope,
                            "global config failed schema validation",
                            vec![err.to_string()],
                            Some("fix config.json or restore from backup"),
                        )),
                    }
                } else {
                    checks.push(DoctorCheck::warn(
                        "config_global",
                        repo_scope,
                        "global config loaded (schema validation skipped)",
                        Vec::new(),
                        Some("restore `.tik/schema` and rerun `tik doctor`"),
                    ));
                }
            }
            Err(err) => checks.push(DoctorCheck::error(
                "config_global",
                repo_scope,
                "global config missing or unreadable",
                vec![err.to_string()],
                Some("restore config.json from backup"),
            )),
        }

        if layout_version == "2.0" {
            let workspace_path = crate::workspace::workspace_path(&self.tik_root);
            match WorkspaceConfig::load(&workspace_path, &schema_dir) {
                Ok(workspace) => {
                    checks.push(DoctorCheck::ok(
                        "workspace",
                        repo_scope,
                        "workspace.json valid",
                    ));
                    let default_root =
                        crate::workspace::project_root(&self.tik_root, &workspace.default_project);
                    match default_root {
                        Ok(root) => {
                            if !root.is_dir() {
                                checks.push(DoctorCheck::error(
                                    "workspace_default_project",
                                    repo_scope,
                                    "default project directory missing",
                                    vec![workspace.default_project.clone()],
                                    Some("run `tik project init <name>` or update workspace.json"),
                                ));
                            }
                        }
                        Err(err) => checks.push(DoctorCheck::error(
                            "workspace_default_project",
                            repo_scope,
                            "default project name invalid",
                            vec![err.to_string()],
                            Some("update workspace.json with a valid project name"),
                        )),
                    }
                }
                Err(err) => checks.push(DoctorCheck::error(
                    "workspace",
                    repo_scope,
                    "workspace.json invalid",
                    vec![err.to_string()],
                    Some("restore workspace.json or run `tik migrate`"),
                )),
            }
        }

        if layout_version == "2.0" {
            let projects_dir = crate::workspace::projects_dir(&self.tik_root);
            if !projects_dir.is_dir() {
                checks.push(DoctorCheck::error(
                    "projects_dir",
                    repo_scope,
                    "projects directory missing",
                    vec![projects_dir.display().to_string()],
                    Some("run `tik migrate` to create the projects layout"),
                ));
            }

            if all_projects && projects_dir.is_dir() {
                match std::fs::read_dir(&projects_dir) {
                    Ok(entries) => {
                        for entry in entries {
                            let entry = match entry {
                                Ok(entry) => entry,
                                Err(err) => {
                                    checks.push(DoctorCheck::error(
                                        "projects_dir",
                                        repo_scope,
                                        "failed to read projects directory entry",
                                        vec![err.to_string()],
                                        None,
                                    ));
                                    continue;
                                }
                            };
                            let is_dir = match entry.file_type() {
                                Ok(file_type) => file_type.is_dir(),
                                Err(err) => {
                                    checks.push(DoctorCheck::error(
                                        "projects_dir",
                                        repo_scope,
                                        "failed to read project entry type",
                                        vec![err.to_string()],
                                        None,
                                    ));
                                    false
                                }
                            };
                            if !is_dir {
                                continue;
                            }
                            let name = entry.file_name().to_string_lossy().to_string();
                            checks.extend(scan_project(
                                &name,
                                &entry.path(),
                                &schema_dir,
                                registry.as_ref(),
                            ));
                        }
                    }
                    Err(err) => checks.push(DoctorCheck::error(
                        "projects_dir",
                        repo_scope,
                        "failed to read projects directory",
                        vec![err.to_string()],
                        None,
                    )),
                }
            } else {
                checks.extend(scan_project(
                    &self.project,
                    &self.data_root,
                    &schema_dir,
                    registry.as_ref(),
                ));
            }
        } else {
            checks.extend(scan_project(
                "default",
                &self.data_root,
                &schema_dir,
                registry.as_ref(),
            ));
        }

        let repo_root = self.root.display().to_string();
        DoctorReport::new(&repo_root, &layout_version, checks)
    }

    pub fn migrate(&mut self) -> Result<MigrationSummary> {
        if self.meta.layout_version == "2.0" {
            return Ok(MigrationSummary {
                migrated_at: timeutil::now_rfc3339()?,
                from_layout: "2.0".to_string(),
                to_layout: "2.0".to_string(),
                project: self.project.clone(),
                moves: Vec::new(),
                created: Vec::new(),
                warnings: vec!["already at layout_version 2.0".to_string()],
            });
        }

        if self.meta.layout_version != "1.0" {
            let msg = format!("unsupported layout_version {}", self.meta.layout_version);
            return Err(TikError::repo_invalid(&msg));
        }

        let _workspace_lock = self.lock_workspace(LockKind::Exclusive)?;
        let _repo_lock = self.lock_repo()?;

        let tik_root = self.tik_root.clone();
        let schema_dir = self.schema_dir();
        schema::ensure_default_schemas(&schema_dir)?;

        let projects_dir = crate::workspace::projects_dir(&tik_root);
        fs::ensure_dir(&projects_dir)?;

        let project = ProjectMeta::new("default", None)?;
        let project_root = crate::workspace::project_root(&tik_root, &project.name)?;
        if project_root.exists() {
            return Err(TikError::Conflict(format!(
                "project root already exists at {}",
                project_root.display()
            )));
        }

        fs::ensure_dir(&project_root)?;

        let mut moves = Vec::new();
        let mut created = Vec::new();
        let mut warnings = Vec::new();

        let global_config = Config::load(&self.global_config_path())?;
        let registry = schema::SchemaRegistry::load(&schema_dir)?;
        registry.validate_config(&global_config)?;

        for dir in ["tickets", "milestones", "index", "tmp"] {
            let src = tik_root.join(dir);
            let dest = project_root.join(dir);
            if src.exists() {
                if dest.exists() {
                    return Err(TikError::Conflict(format!(
                        "destination already exists at {}",
                        dest.display()
                    )));
                }
                std::fs::rename(&src, &dest)
                    .map_err(|err| TikError::io("move project data", err))?;
                moves.push(MigrationMove {
                    from: src.display().to_string(),
                    to: dest.display().to_string(),
                });
            } else {
                fs::ensure_dir(&dest)?;
                created.push(dest.display().to_string());
                warnings.push(format!("{dir} directory missing; created empty"));
            }
        }

        let locks_dir = project_root.join("locks");
        fs::ensure_dir(&locks_dir)?;
        created.push(locks_dir.display().to_string());

        let workspace = WorkspaceConfig::new("default")?;
        let workspace_path = crate::workspace::workspace_path(&tik_root);
        WorkspaceConfig::write(&workspace_path, &workspace, &schema_dir)?;
        created.push(workspace_path.display().to_string());

        ProjectMeta::write(
            &crate::workspace::project_meta_path(&project_root),
            &project,
            &schema_dir,
        )?;
        created.push(
            crate::workspace::project_meta_path(&project_root)
                .display()
                .to_string(),
        );

        Config::write(
            &crate::workspace::project_config_path(&project_root),
            &global_config,
        )?;
        created.push(
            crate::workspace::project_config_path(&project_root)
                .display()
                .to_string(),
        );

        self.meta.layout_version = "2.0".to_string();
        let repo_json = serde_json::to_string_pretty(&self.meta)
            .map_err(|err| TikError::Internal(format!("serialize repo.json: {err}")))?;
        fs::write_string_atomic(&tik_root.join("repo.json"), &repo_json)?;

        self.workspace = Some(workspace);
        self.project = project.name.clone();
        self.data_root = project_root.clone();
        self.config_path = crate::workspace::project_config_path(&project_root);
        self.config = global_config;

        Ok(MigrationSummary {
            migrated_at: timeutil::now_rfc3339()?,
            from_layout: "1.0".to_string(),
            to_layout: "2.0".to_string(),
            project: project.name,
            moves,
            created,
            warnings,
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
        let registry = schema::SchemaRegistry::load(&self.schema_dir())?;
        registry.validate_config(&config)?;
        Config::write(&self.config_path, &config)?;
        self.config = config.clone();
        Ok(config)
    }

    pub fn create_ticket(&self, new_ticket: NewTicket, actor: &str) -> Result<Ticket> {
        let _repo_lock = self.lock_repo()?;
        let ticket = self.ticket_store().create(new_ticket, actor)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(ticket)
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
        self.ticket_store().write_notes_md(id, contents)?;
        let ticket = self.ticket_store().load(id)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(())
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

        if let Ok(ids) =
            self.index_store()
                .list_ids(status.as_ref().map(|value| value.as_str()), offset, limit)
        {
            let mut tickets = Vec::new();
            for id in ids {
                let ticket_id = match TicketId::parse(&id) {
                    Ok(ticket_id) => ticket_id,
                    Err(_) => return self.ticket_store().list_page(status.clone(), offset, limit),
                };
                let ticket = match self.ticket_store().load(&ticket_id) {
                    Ok(ticket) => ticket,
                    Err(_) => return self.ticket_store().list_page(status.clone(), offset, limit),
                };
                if let Some(ref filter) = status {
                    if &ticket.status != filter {
                        return self.ticket_store().list_page(status.clone(), offset, limit);
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
        let milestone = self.milestone_store().create(new_milestone, actor)?;
        self.update_index_for_milestone(&milestone.id)?;
        Ok(milestone)
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
        let milestone = self.milestone_store().close(id, actor, reason)?;
        self.update_index_for_milestone(&milestone.id)?;
        Ok(milestone)
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
        let ticket = self
            .ticket_store()
            .set_milestone(id, milestone_id, actor, reason)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(ticket)
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

    pub fn index_status(&self) -> Result<IndexStatus> {
        let _repo_lock = self.lock_repo_shared()?;
        self.index_store().status()
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
        self.stats_filtered(&ReportFilters::default())
    }

    pub fn stats_filtered(&self, filters: &ReportFilters) -> Result<Stats> {
        let _repo_lock = self.lock_repo_shared()?;
        let tickets = self.ticket_store().list(None)?;
        let milestones = self.milestone_store().list(None)?;
        let tickets = report::filter_tickets(&tickets, filters)?;
        let milestones = report::filter_milestones(&milestones, &tickets, filters);
        Ok(report::compute_stats(&tickets, &milestones))
    }

    pub fn report(&self, recent_limit: usize) -> Result<Report> {
        self.report_filtered_sorted(recent_limit, TicketSort::Updated, &ReportFilters::default())
    }

    pub fn report_sorted(&self, recent_limit: usize, sort: TicketSort) -> Result<Report> {
        self.report_filtered_sorted(recent_limit, sort, &ReportFilters::default())
    }

    pub fn report_filtered_sorted(
        &self,
        recent_limit: usize,
        sort: TicketSort,
        filters: &ReportFilters,
    ) -> Result<Report> {
        let _repo_lock = self.lock_repo_shared()?;
        let tickets = self.ticket_store().list(None)?;
        let milestones = self.milestone_store().list(None)?;
        let tickets = report::filter_tickets(&tickets, filters)?;
        let milestones = report::filter_milestones(&milestones, &tickets, filters);
        report::compute_report_sorted(&tickets, &milestones, recent_limit, sort)
    }

    pub fn burndown(
        &self,
        filters: &ReportFilters,
        group_by: ReportGroupBy,
    ) -> Result<BurndownReport> {
        let _repo_lock = self.lock_repo_shared()?;
        let tickets = self.ticket_store().list(None)?;
        let filters_no_range = filters.without_range();
        let tickets = report::filter_tickets(&tickets, &filters_no_range)?;
        let mut histories = Vec::new();
        for ticket in tickets {
            let events = self.ticket_store().read_events(&ticket.id)?;
            histories.push(TicketHistory { ticket, events });
        }
        report::compute_burndown(&histories, filters, group_by)
    }

    pub fn throughput(
        &self,
        filters: &ReportFilters,
        group_by: ReportGroupBy,
    ) -> Result<ThroughputReport> {
        let _repo_lock = self.lock_repo_shared()?;
        let tickets = self.ticket_store().list(None)?;
        let filters_no_range = filters.without_range();
        let tickets = report::filter_tickets(&tickets, &filters_no_range)?;
        let mut histories = Vec::new();
        for ticket in tickets {
            let events = self.ticket_store().read_events(&ticket.id)?;
            histories.push(TicketHistory { ticket, events });
        }
        report::compute_throughput(&histories, filters, group_by)
    }

    pub fn graph(&self) -> Result<Graph> {
        self.graph_with_options(&GraphOptions::default())
    }

    pub fn graph_with_options(&self, options: &GraphOptions) -> Result<Graph> {
        let _repo_lock = self.lock_repo_shared()?;
        let tickets = self.ticket_store().list(None)?;
        let milestones = self.milestone_store().list(None)?;
        report::compute_graph_scoped(&tickets, &milestones, options)
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
            let milestone = self.milestone_store().import_milestone(
                milestone.milestone,
                milestone.events,
                actor,
            )?;
            self.update_index_for_milestone(&milestone.id)?;
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
            let ticket = self
                .ticket_store()
                .import_ticket(ticket, events, notes_md, actor)?;
            self.update_index_for_ticket(&ticket)?;
            summary.tickets_imported += 1;
        }

        Ok(summary)
    }

    pub fn append_note(&self, id: &TicketId, actor: &str, text: &str) -> Result<Event> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        let event = self.ticket_store().append_note(id, actor, text)?;
        let ticket = self.ticket_store().load(id)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(event)
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
        let ticket = self
            .ticket_store()
            .update_status(id, status, actor, reason)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(ticket)
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
        let ticket = self
            .ticket_store()
            .add_relation(id, kind, target, actor, reason)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(ticket)
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
        let ticket = self
            .ticket_store()
            .remove_relation(id, kind, target, actor, reason)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(ticket)
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
        let ticket = self
            .ticket_store()
            .add_artifact(id, kind, reference, actor, reason)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(ticket)
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
        let ticket = self
            .ticket_store()
            .remove_artifact(id, kind, reference, actor, reason)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(ticket)
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
        let ticket = self
            .ticket_store()
            .add_assignees(id, assignees, actor, reason)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(ticket)
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
        let ticket = self
            .ticket_store()
            .remove_assignees(id, assignees, actor, reason)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(ticket)
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
        let ticket = self
            .ticket_store()
            .set_assignees(id, assignees, actor, reason)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(ticket)
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
        let ticket = self.ticket_store().add_tags(id, tags, actor, reason)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(ticket)
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
        let ticket = self.ticket_store().remove_tags(id, tags, actor, reason)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(ticket)
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
        let ticket = self.ticket_store().set_tags(id, tags, actor, reason)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(ticket)
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
        let ticket = self
            .ticket_store()
            .apply_edit(id, raw_json, actor, reason)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(ticket)
    }

    pub fn touch_notes(&self, id: &TicketId, actor: &str, reason: Option<&str>) -> Result<Event> {
        let _repo_lock = self.lock_repo()?;
        let _ticket_lock = self.lock_ticket(id)?;
        let event = self.ticket_store().touch_notes(id, actor, reason)?;
        let ticket = self.ticket_store().load(id)?;
        self.update_index_for_ticket(&ticket)?;
        Ok(event)
    }

    pub fn read_events(&self, id: &TicketId) -> Result<Vec<Event>> {
        let _repo_lock = self.lock_repo_shared()?;
        self.ticket_store().read_events(id)
    }

    fn ticket_store(&self) -> TicketStore {
        TicketStore::new(self.data_root.clone(), self.schema_dir())
    }

    fn milestone_store(&self) -> MilestoneStore {
        MilestoneStore::new(self.data_root.clone(), self.schema_dir())
    }

    fn index_store(&self) -> IndexStore {
        IndexStore::new(self.data_root.clone())
    }

    fn update_index_for_ticket(&self, ticket: &Ticket) -> Result<()> {
        let notes = self.ticket_store().read_notes_md_if_exists(&ticket.id)?;
        match self.index_store().upsert_ticket(ticket, &notes) {
            Ok(()) => Ok(()),
            Err(TikError::Index(_)) => Ok(()),
            Err(err) => Err(err),
        }
    }

    fn update_index_for_milestone(&self, milestone_id: &MilestoneId) -> Result<()> {
        let tickets = self.ticket_store().list(None)?;
        for ticket in tickets {
            if ticket.milestone_id.as_ref() == Some(milestone_id) {
                self.update_index_for_ticket(&ticket)?;
            }
        }
        match self.index_store().record_milestone_write() {
            Ok(()) => Ok(()),
            Err(TikError::Index(_)) => Ok(()),
            Err(err) => Err(err),
        }
    }

    fn lock_repo(&self) -> Result<LockGuard> {
        fs::ensure_dir(&self.data_root.join("locks"))?;
        let path = self.data_root.join("locks").join("repo.lock");
        lock::acquire_lock(&path, LockKind::Exclusive, Duration::from_secs(2))
    }

    fn lock_repo_shared(&self) -> Result<LockGuard> {
        fs::ensure_dir(&self.data_root.join("locks"))?;
        let path = self.data_root.join("locks").join("repo.lock");
        lock::acquire_lock(&path, LockKind::Shared, Duration::from_secs(2))
    }

    fn lock_workspace(&self, kind: LockKind) -> Result<LockGuard> {
        fs::ensure_dir(&self.tik_root.join("locks"))?;
        let path = self.tik_root.join("locks").join("workspace.lock");
        lock::acquire_lock(&path, kind, Duration::from_secs(2))
    }

    fn schema_dir(&self) -> PathBuf {
        self.tik_root.join("schema")
    }

    fn global_config_path(&self) -> PathBuf {
        self.tik_root.join("config.json")
    }

    fn ensure_projects_enabled(&self) -> Result<()> {
        if self.meta.layout_version != "2.0" {
            return Err(TikError::usage(
                "projects require layout_version 2.0; run `tik migrate`",
            ));
        }
        Ok(())
    }

    fn lock_ticket(&self, id: &TicketId) -> Result<LockGuard> {
        fs::ensure_dir(&self.data_root.join("locks"))?;
        let path = self
            .data_root
            .join("locks")
            .join(format!("{}.lock", id.as_str()));
        lock::acquire_lock(&path, LockKind::Exclusive, Duration::from_secs(2))
    }

    /// Create a backup of the repository.
    pub fn backup(&self, output: &Path) -> Result<BackupSummary> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use tar::Builder;

        let _lock = self.lock_repo_shared()?;
        let started_at = timeutil::now_rfc3339()?;

        let file =
            std::fs::File::create(output).map_err(|err| TikError::io("create backup file", err))?;
        let encoder = GzEncoder::new(file, Compression::default());
        let mut archive = Builder::new(encoder);

        let mut tickets_count = 0usize;
        let mut milestones_count = 0usize;

        // Add the entire .tik directory
        archive
            .append_dir_all(".tik", &self.tik_root)
            .map_err(|err| TikError::io("add .tik to archive", err))?;

        // Count items
        let tickets_dir = self.data_root.join("tickets");
        if tickets_dir.is_dir() {
            for entry in std::fs::read_dir(&tickets_dir)
                .map_err(|err| TikError::io("read tickets dir", err))?
            {
                let entry = entry.map_err(|err| TikError::io("read ticket entry", err))?;
                if entry.path().is_dir() {
                    tickets_count += 1;
                }
            }
        }

        let milestones_dir = self.data_root.join("milestones");
        if milestones_dir.is_dir() {
            for entry in std::fs::read_dir(&milestones_dir)
                .map_err(|err| TikError::io("read milestones dir", err))?
            {
                let entry = entry.map_err(|err| TikError::io("read milestone entry", err))?;
                if entry
                    .path()
                    .extension()
                    .map(|e| e == "json")
                    .unwrap_or(false)
                {
                    milestones_count += 1;
                }
            }
        }

        let encoder = archive
            .into_inner()
            .map_err(|err| TikError::io("finish archive", err))?;
        encoder
            .finish()
            .map_err(|err| TikError::io("finish gzip", err))?;

        let completed_at = timeutil::now_rfc3339()?;
        Ok(BackupSummary {
            started_at,
            completed_at,
            output_path: output.display().to_string(),
            tickets_count,
            milestones_count,
            project: self.project.clone(),
        })
    }

    /// Restore a repository from a backup.
    pub fn restore(root: &Path, input: &Path) -> Result<RestoreSummary> {
        use flate2::read::GzDecoder;
        use tar::Archive;

        let started_at = timeutil::now_rfc3339()?;
        let tik_root = root.join(".tik");

        if tik_root.exists() {
            return Err(TikError::usage(
                ".tik already exists; remove it before restoring",
            ));
        }

        let file =
            std::fs::File::open(input).map_err(|err| TikError::io("open backup file", err))?;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);

        archive
            .unpack(root)
            .map_err(|err| TikError::io("extract archive", err))?;

        // Count items
        let mut tickets_count = 0usize;
        let mut milestones_count = 0usize;

        let tickets_dir = tik_root.join("projects").join("default").join("tickets");
        if tickets_dir.is_dir() {
            for entry in std::fs::read_dir(&tickets_dir)
                .map_err(|err| TikError::io("read tickets dir", err))?
            {
                let entry = entry.map_err(|err| TikError::io("read ticket entry", err))?;
                if entry.path().is_dir() {
                    tickets_count += 1;
                }
            }
        }

        let milestones_dir = tik_root.join("projects").join("default").join("milestones");
        if milestones_dir.is_dir() {
            for entry in std::fs::read_dir(&milestones_dir)
                .map_err(|err| TikError::io("read milestones dir", err))?
            {
                let entry = entry.map_err(|err| TikError::io("read milestone entry", err))?;
                if entry
                    .path()
                    .extension()
                    .map(|e| e == "json")
                    .unwrap_or(false)
                {
                    milestones_count += 1;
                }
            }
        }

        let completed_at = timeutil::now_rfc3339()?;
        Ok(RestoreSummary {
            started_at,
            completed_at,
            input_path: input.display().to_string(),
            tickets_count,
            milestones_count,
        })
    }

    /// Run data migrations to update schema versions.
    pub fn run_data_migrations(
        &self,
        to_version: Option<&str>,
        dry_run: bool,
    ) -> Result<crate::migrate::DataMigrationReport> {
        use crate::migrate::{MigrationRegistry, SchemaVersion};

        let _lock = self.lock_repo_shared()?;
        let registry = MigrationRegistry::new();

        // Determine current version from first ticket
        let tickets_dir = self.data_root.join("tickets");
        let current_version = if tickets_dir.is_dir() {
            let mut version = SchemaVersion::new(1, 0);
            if let Ok(entries) = std::fs::read_dir(&tickets_dir) {
                for entry in entries.flatten() {
                    let ticket_dir = entry.path();
                    if !ticket_dir.is_dir() {
                        continue;
                    }
                    let ticket_path = ticket_dir.join("ticket.json");
                    if ticket_path.is_file() {
                        if let Ok(content) = fs::read_to_string(&ticket_path) {
                            if let Ok(ticket) = serde_json::from_str::<serde_json::Value>(&content)
                            {
                                if let Some(v) =
                                    ticket.get("schema_version").and_then(|v| v.as_str())
                                {
                                    if let Ok(parsed) = SchemaVersion::parse(v) {
                                        version = parsed;
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
            }
            version
        } else {
            SchemaVersion::new(1, 0)
        };

        let target_version = if let Some(v) = to_version {
            SchemaVersion::parse(v)?
        } else {
            registry.latest_version()
        };

        crate::migrate::run_migrations(&self.data_root, &current_version, &target_version, dry_run)
    }

    /// Toggle acceptance criterion completion.
    pub fn toggle_acceptance(
        &self,
        ticket_id: &TicketId,
        index: usize,
        actor: &str,
    ) -> Result<Ticket> {
        use crate::domain::event::Event;
        use crate::schema::SchemaRegistry;

        let _lock = self.lock_ticket(ticket_id)?;
        let mut ticket = self.ticket_store().load(ticket_id)?;

        if index == 0 || index > ticket.acceptance.len() {
            return Err(TikError::NotFound(format!(
                "acceptance criterion {} not found (valid range: 1-{})",
                index,
                ticket.acceptance.len()
            )));
        }

        let now = timeutil::now_rfc3339()?;
        let criterion_idx = index - 1; // Convert to 0-based

        // Get the current text and determine new completion state
        let criterion = &ticket.acceptance[criterion_idx];
        let text = criterion.clone();
        let was_completed = text.starts_with("[x]") || text.starts_with("[X]");

        // Toggle the completion state
        let new_text = if was_completed {
            // Remove completion marker
            text.trim_start_matches("[x] ")
                .trim_start_matches("[X] ")
                .to_string()
        } else {
            // Add completion marker
            format!("[x] {}", text)
        };

        ticket.acceptance[criterion_idx] = new_text;
        ticket.touch(&now);

        // Validate and save ticket
        let ticket_dir = self.data_root.join("tickets").join(ticket_id.as_str());
        let ticket_path = ticket_dir.join("ticket.json");
        let schemas = SchemaRegistry::load(&self.data_root.join("schema"))?;
        schemas.validate_ticket(&ticket)?;

        let ticket_json = serde_json::to_string_pretty(&ticket)
            .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
        crate::fs::write_string_atomic(&ticket_path, &ticket_json)?;

        // Record event
        let event = Event::acceptance_toggled(actor, &now, index, !was_completed);
        schemas.validate_event(&event)?;
        let event_store = EventStore::new(ticket_dir.join("notes.jsonl"));
        event_store.append(&event)?;

        // Update index if enabled
        self.update_index_for_ticket(&ticket)?;

        Ok(ticket)
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

fn init_project_layout(
    project_root: &Path,
    project: &ProjectMeta,
    config: &Config,
    schema_dir: &Path,
) -> Result<()> {
    if project_root.exists() {
        return Err(TikError::Conflict("project directory exists".to_string()));
    }

    fs::ensure_dir(project_root)?;
    fs::ensure_dir(&project_root.join("tickets"))?;
    fs::ensure_dir(&project_root.join("milestones"))?;
    fs::ensure_dir(&project_root.join("index"))?;
    fs::ensure_dir(&project_root.join("locks"))?;
    fs::ensure_dir(&project_root.join("tmp"))?;

    let registry = schema::SchemaRegistry::load(schema_dir)?;
    registry.validate_config(config)?;
    ProjectMeta::write(
        &crate::workspace::project_meta_path(project_root),
        project,
        schema_dir,
    )?;
    Config::write(&crate::workspace::project_config_path(project_root), config)?;

    Ok(())
}

fn scan_project(
    project_name: &str,
    project_root: &Path,
    schema_dir: &Path,
    registry: Option<&schema::SchemaRegistry>,
) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let project_scope = format!("project:{project_name}");
    let validate_schema = registry.is_some();

    let lock_dir = project_root.join("locks");
    let _lock = if lock_dir.is_dir() {
        match lock_project_shared(project_root) {
            Ok(guard) => Some(guard),
            Err(err) => {
                checks.push(DoctorCheck::error(
                    "project_lock",
                    &project_scope,
                    "failed to acquire shared project lock",
                    vec![err.to_string()],
                    Some("close other tik processes or remove stale lock files"),
                ));
                return checks;
            }
        }
    } else {
        checks.push(DoctorCheck::warn(
            "project_lock",
            &project_scope,
            "locks directory missing; skipping lock",
            vec![lock_dir.display().to_string()],
            Some("recreate the locks directory"),
        ));
        None
    };

    if !validate_schema {
        checks.push(DoctorCheck::warn(
            "schema_validation_skipped",
            &project_scope,
            "schema validation skipped",
            Vec::new(),
            Some("restore `.tik/schema` and rerun `tik doctor`"),
        ));
    }

    let meta_path = crate::workspace::project_meta_path(project_root);
    if !meta_path.is_file() {
        checks.push(DoctorCheck::error(
            "project_meta",
            &project_scope,
            "project.json missing",
            vec![meta_path.display().to_string()],
            Some("restore project.json from backup"),
        ));
    } else {
        match ProjectMeta::load(&meta_path, schema_dir) {
            Ok(_) => checks.push(DoctorCheck::ok(
                "project_meta",
                &project_scope,
                "project metadata valid",
            )),
            Err(err) => checks.push(DoctorCheck::error(
                "project_meta",
                &project_scope,
                "project metadata invalid",
                vec![err.to_string()],
                Some("fix project.json or restore from backup"),
            )),
        }
    }

    let config_path = crate::workspace::project_config_path(project_root);
    if !config_path.is_file() {
        checks.push(DoctorCheck::warn(
            "project_config",
            &project_scope,
            "project config missing; global config will be used",
            vec![config_path.display().to_string()],
            None,
        ));
    } else {
        match Config::load(&config_path) {
            Ok(config) => {
                if let Some(registry) = registry {
                    if let Err(err) = registry.validate_config(&config) {
                        checks.push(DoctorCheck::error(
                            "project_config",
                            &project_scope,
                            "project config failed schema validation",
                            vec![err.to_string()],
                            Some("fix project config or remove it to use global defaults"),
                        ));
                    } else {
                        checks.push(DoctorCheck::ok(
                            "project_config",
                            &project_scope,
                            "project config valid",
                        ));
                    }
                }
            }
            Err(err) => checks.push(DoctorCheck::error(
                "project_config",
                &project_scope,
                "project config unreadable",
                vec![err.to_string()],
                Some("restore config.json from backup"),
            )),
        }
    }

    let mut missing_dirs = Vec::new();
    for dir in ["tickets", "milestones", "index", "locks", "tmp"] {
        if !project_root.join(dir).is_dir() {
            missing_dirs.push(dir.to_string());
        }
    }
    if missing_dirs.is_empty() {
        checks.push(DoctorCheck::ok(
            "project_dirs",
            &project_scope,
            "project directories present",
        ));
    } else {
        checks.push(DoctorCheck::warn(
            "project_dirs",
            &project_scope,
            "project directories missing",
            missing_dirs,
            Some("recreate missing directories or run `tik migrate`"),
        ));
    }

    let index_dir = project_root.join("index");
    if index_dir.is_dir() {
        let fts = index_dir.join("fts.sqlite").is_file();
        let jsonl = index_dir.join("tickets.jsonl").is_file();
        if fts || jsonl {
            if fts && jsonl {
                checks.push(DoctorCheck::ok(
                    "index_present",
                    &project_scope,
                    "search index present",
                ));
            } else {
                checks.push(DoctorCheck::warn(
                    "index_present",
                    &project_scope,
                    "index incomplete",
                    vec![
                        format!("fts.sqlite={fts}"),
                        format!("tickets.jsonl={jsonl}"),
                    ],
                    Some("run `tik index rebuild` to regenerate the index"),
                ));
            }
        } else {
            checks.push(DoctorCheck::warn(
                "index_present",
                &project_scope,
                "search index missing",
                Vec::new(),
                Some("run `tik index rebuild`"),
            ));
        }
    }

    let mut ticket_ids = HashSet::new();
    let mut tickets = Vec::new();
    let mut ticket_errors = 0usize;
    let mut ticket_warnings = 0usize;
    let tickets_dir = project_root.join("tickets");
    let ticket_store = TicketStore::new(project_root.to_path_buf(), schema_dir.to_path_buf());

    if tickets_dir.is_dir() {
        let dir_entries = match std::fs::read_dir(&tickets_dir) {
            Ok(entries) => Some(entries),
            Err(err) => {
                ticket_errors += 1;
                checks.push(DoctorCheck::error(
                    "tickets_scan",
                    &project_scope,
                    "failed to read tickets directory",
                    vec![err.to_string()],
                    None,
                ));
                None
            }
        };

        if let Some(dir_entries) = dir_entries {
            for entry in dir_entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(err) => {
                        ticket_errors += 1;
                        checks.push(DoctorCheck::error(
                            "ticket_entry",
                            &project_scope,
                            "failed to read ticket directory entry",
                            vec![err.to_string()],
                            None,
                        ));
                        continue;
                    }
                };
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(err) => {
                        ticket_errors += 1;
                        checks.push(DoctorCheck::error(
                            "ticket_entry",
                            &project_scope,
                            "failed to read ticket entry type",
                            vec![err.to_string()],
                            None,
                        ));
                        continue;
                    }
                };
                if !file_type.is_dir() {
                    continue;
                }

                let dir_name = entry.file_name().to_string_lossy().to_string();
                let ticket_scope = format!("ticket:{dir_name}");
                let parsed_dir_id = TicketId::parse(&dir_name).ok();
                if parsed_dir_id.is_none() {
                    ticket_warnings += 1;
                    checks.push(DoctorCheck::warn(
                        "ticket_dir_name",
                        &ticket_scope,
                        "ticket directory name invalid",
                        vec![dir_name.clone()],
                        Some("rename the directory to match the ticket id"),
                    ));
                }

                let ticket_path = entry.path().join("ticket.json");
                if !ticket_path.is_file() {
                    ticket_errors += 1;
                    checks.push(DoctorCheck::error(
                        "ticket_json",
                        &ticket_scope,
                        "ticket.json missing",
                        vec![ticket_path.display().to_string()],
                        Some("restore ticket.json from backup"),
                    ));
                    continue;
                }

                let raw = match fs::read_to_string(&ticket_path) {
                    Ok(raw) => raw,
                    Err(err) => {
                        ticket_errors += 1;
                        checks.push(DoctorCheck::error(
                            "ticket_json",
                            &ticket_scope,
                            "ticket.json unreadable",
                            vec![err.to_string()],
                            Some("restore ticket.json from backup"),
                        ));
                        continue;
                    }
                };

                let ticket: Ticket = match serde_json::from_str(&raw) {
                    Ok(ticket) => ticket,
                    Err(err) => {
                        ticket_errors += 1;
                        checks.push(DoctorCheck::error(
                            "ticket_json",
                            &ticket_scope,
                            "ticket.json invalid",
                            vec![err.to_string()],
                            Some("fix the JSON or restore from backup"),
                        ));
                        continue;
                    }
                };

                if let Some(registry) = registry {
                    if let Err(err) = registry.validate_ticket(&ticket) {
                        ticket_errors += 1;
                        checks.push(DoctorCheck::error(
                            "ticket_schema",
                            &ticket_scope,
                            "ticket failed schema validation",
                            vec![err.to_string()],
                            Some("fix ticket.json fields or restore from backup"),
                        ));
                    }
                }

                if let Some(dir_id) = parsed_dir_id {
                    if dir_id != ticket.id {
                        ticket_errors += 1;
                        checks.push(DoctorCheck::error(
                            "ticket_id_mismatch",
                            &ticket_scope,
                            "ticket id does not match directory name",
                            vec![format!("dir={dir_id} ticket={}", ticket.id.as_str())],
                            Some("rename the directory or update ticket.json"),
                        ));
                    }
                }

                if !ticket_ids.insert(ticket.id.as_str().to_string()) {
                    ticket_errors += 1;
                    checks.push(DoctorCheck::error(
                        "ticket_duplicate",
                        &ticket_scope,
                        "duplicate ticket id detected",
                        vec![ticket.id.as_str().to_string()],
                        Some("deduplicate tickets or fix ids"),
                    ));
                }

                let notes_log = entry.path().join("notes.jsonl");
                if !notes_log.is_file() {
                    ticket_errors += 1;
                    checks.push(DoctorCheck::error(
                        "ticket_events",
                        &ticket_scope,
                        "notes.jsonl missing",
                        vec![notes_log.display().to_string()],
                        Some("restore notes.jsonl from backup"),
                    ));
                } else {
                    match EventStore::new(notes_log.clone()).read_all() {
                        Ok(events) => {
                            if events.is_empty() {
                                ticket_errors += 1;
                                checks.push(DoctorCheck::error(
                                    "ticket_events",
                                    &ticket_scope,
                                    "notes.jsonl is empty",
                                    Vec::new(),
                                    Some("restore notes.jsonl from backup"),
                                ));
                            } else {
                                if let Some(registry) = registry {
                                    for event in &events {
                                        if let Err(err) = registry.validate_event(event) {
                                            ticket_errors += 1;
                                            checks.push(DoctorCheck::error(
                                                "ticket_event_schema",
                                                &ticket_scope,
                                                "event failed schema validation",
                                                vec![err.to_string()],
                                                Some("fix events or restore from backup"),
                                            ));
                                            break;
                                        }
                                    }
                                }
                                if validate_schema {
                                    match ticket_store.rebuild_from_events(&ticket.id, &events) {
                                        Ok(rebuilt) => {
                                            if rebuilt != ticket {
                                                ticket_errors += 1;
                                                checks.push(DoctorCheck::error(
                                                "ticket_event_consistency",
                                                &ticket_scope,
                                                "ticket does not match event log",
                                                Vec::new(),
                                                Some(
                                                    "restore ticket.json or rebuild from event log",
                                                ),
                                            ));
                                            }
                                        }
                                        Err(err) => {
                                            ticket_errors += 1;
                                            checks.push(DoctorCheck::error(
                                                "ticket_event_consistency",
                                                &ticket_scope,
                                                "failed to rebuild ticket from events",
                                                vec![err.to_string()],
                                                Some("restore notes.jsonl from backup"),
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            ticket_errors += 1;
                            checks.push(DoctorCheck::error(
                                "ticket_events",
                                &ticket_scope,
                                "failed to read notes.jsonl",
                                vec![err.to_string()],
                                Some("fix notes.jsonl or restore from backup"),
                            ));
                        }
                    }
                }

                let notes_md = entry.path().join("notes.md");
                if !notes_md.is_file() {
                    ticket_warnings += 1;
                    checks.push(DoctorCheck::warn(
                        "ticket_notes_md",
                        &ticket_scope,
                        "notes.md missing",
                        vec![notes_md.display().to_string()],
                        Some("run `tik edit --notes` to regenerate notes.md"),
                    ));
                }

                tickets.push(ticket);
            }
        }
    } else {
        ticket_warnings += 1;
        checks.push(DoctorCheck::warn(
            "tickets_scan",
            &project_scope,
            "tickets directory missing",
            vec![tickets_dir.display().to_string()],
            Some("recreate the tickets directory or run `tik migrate`"),
        ));
    }

    let ticket_summary_status = if ticket_errors > 0 {
        DoctorStatus::Error
    } else if ticket_warnings > 0 {
        DoctorStatus::Warn
    } else {
        DoctorStatus::Ok
    };
    checks.push(DoctorCheck {
        id: "tickets_scan".to_string(),
        status: ticket_summary_status,
        scope: project_scope.clone(),
        message: format!("scanned {} tickets", tickets.len()),
        details: if ticket_errors > 0 || ticket_warnings > 0 {
            vec![format!("errors={ticket_errors} warnings={ticket_warnings}")]
        } else {
            Vec::new()
        },
        hint: None,
    });

    let milestones_dir = project_root.join("milestones");
    let mut milestone_ids = HashSet::new();
    let mut milestone_errors = 0usize;
    let mut milestone_warnings = 0usize;
    let mut milestones = Vec::new();

    if milestones_dir.is_dir() {
        let dir_entries = match std::fs::read_dir(&milestones_dir) {
            Ok(entries) => Some(entries),
            Err(err) => {
                milestone_errors += 1;
                checks.push(DoctorCheck::error(
                    "milestones_scan",
                    &project_scope,
                    "failed to read milestones directory",
                    vec![err.to_string()],
                    None,
                ));
                None
            }
        };

        if let Some(dir_entries) = dir_entries {
            for entry in dir_entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(err) => {
                        milestone_errors += 1;
                        checks.push(DoctorCheck::error(
                            "milestone_entry",
                            &project_scope,
                            "failed to read milestone entry",
                            vec![err.to_string()],
                            None,
                        ));
                        continue;
                    }
                };
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(err) => {
                        milestone_errors += 1;
                        checks.push(DoctorCheck::error(
                            "milestone_entry",
                            &project_scope,
                            "failed to read milestone entry type",
                            vec![err.to_string()],
                            None,
                        ));
                        continue;
                    }
                };
                if !file_type.is_file() {
                    continue;
                }

                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }

                let file_name = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("");
                let milestone_scope = format!("milestone:{file_name}");
                let parsed_id = MilestoneId::parse(file_name).ok();
                if parsed_id.is_none() {
                    milestone_warnings += 1;
                    checks.push(DoctorCheck::warn(
                        "milestone_file_name",
                        &milestone_scope,
                        "milestone file name invalid",
                        vec![file_name.to_string()],
                        Some("rename the milestone file to match its id"),
                    ));
                }

                let raw = match fs::read_to_string(&path) {
                    Ok(raw) => raw,
                    Err(err) => {
                        milestone_errors += 1;
                        checks.push(DoctorCheck::error(
                            "milestone_json",
                            &milestone_scope,
                            "milestone file unreadable",
                            vec![err.to_string()],
                            Some("restore milestone file from backup"),
                        ));
                        continue;
                    }
                };
                let milestone: Milestone = match serde_json::from_str(&raw) {
                    Ok(milestone) => milestone,
                    Err(err) => {
                        milestone_errors += 1;
                        checks.push(DoctorCheck::error(
                            "milestone_json",
                            &milestone_scope,
                            "milestone file invalid",
                            vec![err.to_string()],
                            Some("fix milestone JSON or restore from backup"),
                        ));
                        continue;
                    }
                };

                if let Some(registry) = registry {
                    if let Err(err) = registry.validate_milestone(&milestone) {
                        milestone_errors += 1;
                        checks.push(DoctorCheck::error(
                            "milestone_schema",
                            &milestone_scope,
                            "milestone failed schema validation",
                            vec![err.to_string()],
                            Some("fix milestone fields or restore from backup"),
                        ));
                    }
                }

                if let Some(file_id) = parsed_id {
                    if file_id != milestone.id {
                        milestone_errors += 1;
                        checks.push(DoctorCheck::error(
                            "milestone_id_mismatch",
                            &milestone_scope,
                            "milestone id does not match file name",
                            vec![format!(
                                "file={file_id} milestone={}",
                                milestone.id.as_str()
                            )],
                            Some("rename the file or update milestone id"),
                        ));
                    }
                }

                milestone_ids.insert(milestone.id.as_str().to_string());
                milestones.push(milestone.clone());

                let log_path = milestones_dir.join(format!("{}.jsonl", milestone.id.as_str()));
                if !log_path.is_file() {
                    milestone_warnings += 1;
                    checks.push(DoctorCheck::warn(
                        "milestone_events",
                        &milestone_scope,
                        "milestone events log missing",
                        vec![log_path.display().to_string()],
                        Some("restore milestone event log from backup"),
                    ));
                } else {
                    match EventStore::new(log_path.clone()).read_all() {
                        Ok(events) => {
                            if let Some(registry) = registry {
                                for event in &events {
                                    if let Err(err) = registry.validate_event(event) {
                                        milestone_errors += 1;
                                        checks.push(DoctorCheck::error(
                                            "milestone_event_schema",
                                            &milestone_scope,
                                            "milestone event failed schema validation",
                                            vec![err.to_string()],
                                            Some("fix event log or restore from backup"),
                                        ));
                                        break;
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            milestone_errors += 1;
                            checks.push(DoctorCheck::error(
                                "milestone_events",
                                &milestone_scope,
                                "failed to read milestone events",
                                vec![err.to_string()],
                                Some("fix milestone events log or restore from backup"),
                            ));
                        }
                    }
                }
            }
        }
    } else {
        milestone_warnings += 1;
        checks.push(DoctorCheck::warn(
            "milestones_scan",
            &project_scope,
            "milestones directory missing",
            vec![milestones_dir.display().to_string()],
            Some("recreate the milestones directory or run `tik migrate`"),
        ));
    }

    let milestone_summary_status = if milestone_errors > 0 {
        DoctorStatus::Error
    } else if milestone_warnings > 0 {
        DoctorStatus::Warn
    } else {
        DoctorStatus::Ok
    };
    checks.push(DoctorCheck {
        id: "milestones_scan".to_string(),
        status: milestone_summary_status,
        scope: project_scope.clone(),
        message: format!("scanned {} milestones", milestones.len()),
        details: if milestone_errors > 0 || milestone_warnings > 0 {
            vec![format!(
                "errors={milestone_errors} warnings={milestone_warnings}"
            )]
        } else {
            Vec::new()
        },
        hint: None,
    });

    for ticket in &tickets {
        if let Some(milestone_id) = &ticket.milestone_id {
            if !milestone_ids.contains(milestone_id.as_str()) {
                checks.push(DoctorCheck::error(
                    "ticket_milestone",
                    &format!("ticket:{}", ticket.id.as_str()),
                    "ticket milestone not found",
                    vec![milestone_id.as_str().to_string()],
                    Some("run `tik milestone set --clear` or recreate the milestone"),
                ));
            }
        }
        for relation in &ticket.relations {
            if !ticket_ids.contains(relation.id.as_str()) {
                checks.push(DoctorCheck::warn(
                    "ticket_relation",
                    &format!("ticket:{}", ticket.id.as_str()),
                    "related ticket not found",
                    vec![relation.id.as_str().to_string()],
                    Some("remove the relation or recreate the target ticket"),
                ));
            }
        }
    }

    checks
}

fn lock_project_shared(project_root: &Path) -> Result<LockGuard> {
    let path = project_root.join("locks").join("repo.lock");
    lock::acquire_lock(&path, LockKind::Shared, Duration::from_secs(2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ticket::NewTicket;
    use crate::domain::ticket::RelationType;
    use crate::search::SearchQuery;
    use tempfile::tempdir;

    #[test]
    fn init_creates_layout() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        let tik_root = dir.path().join(".tik");
        assert!(tik_root.is_dir());
        assert!(tik_root.join("schema").is_dir());
        assert!(tik_root.join("projects").is_dir());
        assert!(tik_root.join("repo.json").is_file());
        assert!(tik_root.join("config.json").is_file());
        assert!(tik_root.join("workspace.json").is_file());
        let project_root = tik_root.join("projects").join("default");
        assert!(project_root.is_dir());
        assert!(project_root.join("tickets").is_dir());
        assert!(project_root.join("milestones").is_dir());
        assert!(project_root.join("project.json").is_file());
        assert!(project_root.join("config.json").is_file());
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
    fn project_list_includes_default() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        let projects = repo.project_list().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "default");
    }

    #[test]
    fn project_init_adds_project() {
        let dir = tempdir().unwrap();
        let mut repo = Repo::init(dir.path(), "0.1.0").unwrap();
        let project = repo.project_init("alpha", Some("Project alpha")).unwrap();
        assert_eq!(project.name, "alpha");
        let projects = repo.project_list().unwrap();
        assert!(projects.iter().any(|item| item.name == "alpha"));
    }

    #[test]
    fn project_select_updates_workspace_default() {
        let dir = tempdir().unwrap();
        let mut repo = Repo::init(dir.path(), "0.1.0").unwrap();
        repo.project_init("alpha", None).unwrap();
        let selected = repo.project_select("alpha").unwrap();
        assert_eq!(selected.name, "alpha");
        let status = repo.status().unwrap();
        assert_eq!(status.project, "alpha");

        let reopened = Repo::open(dir.path()).unwrap();
        let reopened_status = reopened.status().unwrap();
        assert_eq!(reopened_status.project, "alpha");
    }

    #[test]
    fn doctor_reports_index_warning() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        let report = repo.doctor(false).unwrap();
        assert_eq!(report.summary.errors, 0);
        assert!(report.summary.warnings >= 1);
        assert!(report
            .checks
            .iter()
            .any(|check| check.id == "index_present"));
    }

    #[test]
    fn migrate_layout_v1_to_v2() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        repo.create_ticket(
            NewTicket {
                title: "Legacy".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "human",
        )
        .unwrap();

        let tik_root = dir.path().join(".tik");
        let project_root = tik_root.join("projects").join("default");
        for dir_name in ["tickets", "milestones", "index", "tmp"] {
            let src = project_root.join(dir_name);
            if src.exists() {
                std::fs::rename(&src, tik_root.join(dir_name)).unwrap();
            }
        }
        std::fs::remove_file(tik_root.join("workspace.json")).unwrap();
        std::fs::remove_dir_all(tik_root.join("projects")).unwrap();

        let meta_path = tik_root.join("repo.json");
        let raw = fs::read_to_string(&meta_path).unwrap();
        let mut meta: RepoMeta = serde_json::from_str(&raw).unwrap();
        meta.layout_version = "1.0".to_string();
        let raw = serde_json::to_string_pretty(&meta).unwrap();
        fs::write_string_atomic(&meta_path, &raw).unwrap();

        let mut legacy = Repo::open(dir.path()).unwrap();
        assert_eq!(legacy.meta.layout_version, "1.0");

        let summary = legacy.migrate().unwrap();
        assert_eq!(summary.from_layout, "1.0");
        assert_eq!(summary.to_layout, "2.0");

        let migrated = Repo::open(dir.path()).unwrap();
        let status = migrated.status().unwrap();
        assert_eq!(status.project, "default");
        assert!(PathBuf::from(status.project_root).join("tickets").is_dir());
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
    fn burndown_uses_repo_history() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        let store = repo.ticket_store();

        let mut ticket_a = Ticket::new(
            NewTicket {
                title: "A".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        ticket_a.id = TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        ticket_a.status = TicketStatus::Closed;
        ticket_a.updated_at = "2026-01-03T12:00:00Z".to_string();
        ticket_a.closed_at = Some(ticket_a.updated_at.clone());

        let mut ticket_b = Ticket::new(
            NewTicket {
                title: "B".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "2026-01-02T00:00:00Z",
        );
        ticket_b.id = TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAW").unwrap();
        ticket_b.status = TicketStatus::Closed;
        ticket_b.updated_at = "2026-01-04T12:00:00Z".to_string();
        ticket_b.closed_at = Some(ticket_b.updated_at.clone());

        let mut created_a = ticket_a.clone();
        created_a.status = TicketStatus::Open;
        created_a.updated_at = created_a.created_at.clone();
        created_a.closed_at = None;
        let mut created_b = ticket_b.clone();
        created_b.status = TicketStatus::Open;
        created_b.updated_at = created_b.created_at.clone();
        created_b.closed_at = None;

        let events_a = vec![
            Event::new(
                "created",
                "human",
                "2026-01-01T00:00:00Z",
                serde_json::json!({"ticket": created_a}),
            ),
            Event::new(
                "status_change",
                "human",
                "2026-01-03T12:00:00Z",
                serde_json::json!({"from": "open", "to": "closed"}),
            ),
        ];
        let events_b = vec![
            Event::new(
                "created",
                "human",
                "2026-01-02T00:00:00Z",
                serde_json::json!({"ticket": created_b}),
            ),
            Event::new(
                "status_change",
                "human",
                "2026-01-04T12:00:00Z",
                serde_json::json!({"from": "open", "to": "closed"}),
            ),
        ];

        store
            .import_ticket(ticket_a, events_a, None, "human")
            .unwrap();
        store
            .import_ticket(ticket_b, events_b, None, "human")
            .unwrap();

        let filters = ReportFilters::parse(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Some("2026-01-01"),
            Some("2026-01-04"),
        )
        .unwrap();
        let report = repo.burndown(&filters, ReportGroupBy::Day).unwrap();
        let counts: Vec<usize> = report
            .points
            .iter()
            .map(|point| point.open_tickets)
            .collect();
        assert_eq!(counts, vec![1, 2, 1, 0]);
    }

    #[test]
    fn throughput_uses_repo_history() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0").unwrap();
        let store = repo.ticket_store();

        let mut ticket = Ticket::new(
            NewTicket {
                title: "A".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        ticket.id = TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        ticket.status = TicketStatus::Closed;
        ticket.updated_at = "2026-01-02T12:00:00Z".to_string();
        ticket.closed_at = Some(ticket.updated_at.clone());

        let mut created_ticket = ticket.clone();
        created_ticket.status = TicketStatus::Open;
        created_ticket.updated_at = created_ticket.created_at.clone();
        created_ticket.closed_at = None;

        let events = vec![
            Event::new(
                "created",
                "human",
                "2026-01-01T00:00:00Z",
                serde_json::json!({"ticket": created_ticket}),
            ),
            Event::new(
                "status_change",
                "human",
                "2026-01-02T12:00:00Z",
                serde_json::json!({"from": "open", "to": "closed"}),
            ),
        ];

        store.import_ticket(ticket, events, None, "human").unwrap();

        let filters = ReportFilters::parse(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Some("2026-01-01"),
            Some("2026-01-02"),
        )
        .unwrap();
        let report = repo.throughput(&filters, ReportGroupBy::Day).unwrap();
        let counts: Vec<usize> = report
            .points
            .iter()
            .map(|point| point.closed_tickets)
            .collect();
        assert_eq!(counts, vec![0, 1]);
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
