use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::domain::ticket::Ticket;
use crate::fs as tikfs;
use crate::search::{DateBound, DateFilter, SearchQuery};
use crate::timeutil;
use crate::{Result, TikError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSummary {
    pub indexed_at: String,
    pub ticket_count: usize,
    pub index_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexStatus {
    pub index_present: bool,
    pub fts_path: String,
    pub tickets_jsonl_path: String,
    pub metadata_path: String,
    pub indexed_at: Option<String>,
    pub ticket_count: Option<usize>,
    pub last_ticket_write: Option<String>,
    pub last_milestone_write: Option<String>,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexMetadata {
    schema_version: String,
    indexed_at: String,
    ticket_count: usize,
    last_ticket_write: String,
    last_milestone_write: String,
}

#[derive(Debug, Clone)]
pub struct IndexStore {
    data_root: PathBuf,
}

impl IndexStore {
    pub fn new(data_root: PathBuf) -> IndexStore {
        IndexStore { data_root }
    }

    pub fn rebuild(
        &self,
        tickets: &[Ticket],
        notes: &HashMap<String, String>,
    ) -> Result<IndexSummary> {
        let index_dir = self.index_dir();
        tikfs::ensure_dir(&index_dir)?;
        let index_path = self.index_path();
        if index_path.exists() {
            tikfs::remove_file_safe(&index_path)?;
        }

        let tmp = NamedTempFile::new_in(&index_dir)
            .map_err(|err| TikError::Index(format!("create temp index: {err}")))?;
        let tmp_path = tmp.into_temp_path();
        let tmp_path_ref: &Path = tmp_path.as_ref();
        let mut conn = Connection::open(tmp_path_ref)
            .map_err(|err| TikError::Index(format!("open index: {err}")))?;
        conn.query_row("PRAGMA journal_mode=DELETE", [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|err| TikError::Index(format!("init index: {err}")))?;
        conn.execute_batch(
            "PRAGMA synchronous=NORMAL;
             PRAGMA temp_store=MEMORY;
             DROP TABLE IF EXISTS tickets;
             DROP TABLE IF EXISTS tickets_fts;
             CREATE TABLE tickets (
               id TEXT NOT NULL UNIQUE,
               title TEXT NOT NULL,
               summary TEXT NOT NULL,
               description TEXT NOT NULL,
               status TEXT NOT NULL,
               type TEXT NOT NULL,
               priority TEXT NOT NULL,
               severity TEXT NOT NULL,
               assignees TEXT NOT NULL,
               milestone_id TEXT,
               tags TEXT NOT NULL,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL,
               closed_at TEXT,
               due_at TEXT,
               notes TEXT NOT NULL
             );
             CREATE VIRTUAL TABLE tickets_fts USING fts5(
               title, summary, description, notes, tags, assignees,
               content='tickets', content_rowid='rowid'
             );",
        )
        .map_err(|err| TikError::Index(format!("init index: {err}")))?;

        let tx = conn
            .transaction()
            .map_err(|err| TikError::Index(format!("index transaction: {err}")))?;
        let mut jsonl_lines = Vec::new();
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO tickets (
                        rowid, id, title, summary, description, status, type, priority, severity,
                        assignees, milestone_id, tags, created_at, updated_at, closed_at, due_at, notes
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                )
                .map_err(|err| TikError::Index(format!("prepare index insert: {err}")))?;
            let mut fts_stmt = tx
                .prepare(
                    "INSERT INTO tickets_fts (
                        rowid, title, summary, description, notes, tags, assignees
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .map_err(|err| TikError::Index(format!("prepare fts insert: {err}")))?;

            for (idx, ticket) in tickets.iter().enumerate() {
                let rowid = (idx + 1) as i64;
                let notes_text = notes.get(ticket.id.as_str()).cloned().unwrap_or_default();
                let tags = normalize_list(&ticket.tags);
                let assignees = normalize_list(&ticket.assignees);
                let milestone_id = ticket.milestone_id.as_ref().map(|id| id.as_str());

                stmt.execute(params![
                    rowid,
                    ticket.id.as_str(),
                    ticket.title.as_str(),
                    ticket.summary.as_str(),
                    ticket.description.as_str(),
                    ticket.status.as_str(),
                    ticket.kind.as_str(),
                    ticket.priority.as_str(),
                    ticket.severity.as_str(),
                    assignees.as_str(),
                    milestone_id,
                    tags.as_str(),
                    ticket.created_at.as_str(),
                    ticket.updated_at.as_str(),
                    ticket.closed_at.as_deref(),
                    ticket.due_at.as_deref(),
                    notes_text.as_str(),
                ])
                .map_err(|err| TikError::Index(format!("index insert: {err}")))?;

                fts_stmt
                    .execute(params![
                        rowid,
                        ticket.title.as_str(),
                        ticket.summary.as_str(),
                        ticket.description.as_str(),
                        notes_text.as_str(),
                        tags.as_str(),
                        assignees.as_str(),
                    ])
                    .map_err(|err| TikError::Index(format!("fts insert: {err}")))?;

                let entry = IndexTicket::from_ticket(ticket, &notes_text);
                let line = serde_json::to_string(&entry)
                    .map_err(|err| TikError::Index(format!("index jsonl: {err}")))?;
                jsonl_lines.push(line);
            }
        }

        tx.commit()
            .map_err(|err| TikError::Index(format!("commit index: {err}")))?;

        drop(conn);
        tikfs::remove_file_safe(&index_dir.join("fts.sqlite-wal"))?;
        tikfs::remove_file_safe(&index_dir.join("fts.sqlite-shm"))?;
        tikfs::atomic_replace_path(tmp_path.as_ref(), &index_path)?;

        let jsonl_path = index_dir.join("tickets.jsonl");
        let mut jsonl = jsonl_lines.join("\n");
        if !jsonl.is_empty() {
            jsonl.push('\n');
        }
        tikfs::write_string_atomic(&jsonl_path, &jsonl)?;

        let indexed_at = timeutil::now_rfc3339()?;
        let metadata = IndexMetadata {
            schema_version: "1.0".to_string(),
            indexed_at: indexed_at.clone(),
            ticket_count: tickets.len(),
            last_ticket_write: indexed_at.clone(),
            last_milestone_write: indexed_at.clone(),
        };
        self.write_metadata(&metadata)?;

        Ok(IndexSummary {
            indexed_at,
            ticket_count: tickets.len(),
            index_path: index_path.display().to_string(),
        })
    }

    pub fn upsert_ticket(&self, ticket: &Ticket, notes: &str) -> Result<()> {
        let index_path = self.index_path();
        if !index_path.is_file() {
            return Err(TikError::Index("index missing".to_string()));
        }

        let mut conn = Connection::open(&index_path)
            .map_err(|err| TikError::Index(format!("open index: {err}")))?;
        let tx = conn
            .transaction()
            .map_err(|err| TikError::Index(format!("index transaction: {err}")))?;

        let existing = tx
            .query_row(
                "SELECT rowid, title, summary, description, notes, tags, assignees
                 FROM tickets WHERE id = ?1",
                params![ticket.id.as_str()],
                |row| {
                    Ok(ExistingIndexRow {
                        rowid: row.get(0)?,
                        title: row.get(1)?,
                        summary: row.get(2)?,
                        description: row.get(3)?,
                        notes: row.get(4)?,
                        tags: row.get(5)?,
                        assignees: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(|err| TikError::Index(format!("fetch existing ticket: {err}")))?;

        let tags = normalize_list(&ticket.tags);
        let assignees = normalize_list(&ticket.assignees);
        let milestone_id = ticket.milestone_id.as_ref().map(|id| id.as_str());

        tx.execute(
            "INSERT INTO tickets (
                id, title, summary, description, status, type, priority, severity, assignees,
                milestone_id, tags, created_at, updated_at, closed_at, due_at, notes
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(id) DO UPDATE SET
                title=excluded.title,
                summary=excluded.summary,
                description=excluded.description,
                status=excluded.status,
                type=excluded.type,
                priority=excluded.priority,
                severity=excluded.severity,
                assignees=excluded.assignees,
                milestone_id=excluded.milestone_id,
                tags=excluded.tags,
                created_at=excluded.created_at,
                updated_at=excluded.updated_at,
                closed_at=excluded.closed_at,
                due_at=excluded.due_at,
                notes=excluded.notes",
            params![
                ticket.id.as_str(),
                ticket.title.as_str(),
                ticket.summary.as_str(),
                ticket.description.as_str(),
                ticket.status.as_str(),
                ticket.kind.as_str(),
                ticket.priority.as_str(),
                ticket.severity.as_str(),
                assignees.as_str(),
                milestone_id,
                tags.as_str(),
                ticket.created_at.as_str(),
                ticket.updated_at.as_str(),
                ticket.closed_at.as_deref(),
                ticket.due_at.as_deref(),
                notes,
            ],
        )
        .map_err(|err| TikError::Index(format!("upsert ticket: {err}")))?;

        let rowid: i64 = tx
            .query_row(
                "SELECT rowid FROM tickets WHERE id = ?1",
                params![ticket.id.as_str()],
                |row| row.get(0),
            )
            .map_err(|err| TikError::Index(format!("fetch rowid: {err}")))?;

        if let Some(existing) = existing {
            tx.execute(
                "INSERT INTO tickets_fts(
                    tickets_fts, rowid, title, summary, description, notes, tags, assignees
                 ) VALUES('delete', ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    existing.rowid,
                    existing.title.as_str(),
                    existing.summary.as_str(),
                    existing.description.as_str(),
                    existing.notes.as_str(),
                    existing.tags.as_str(),
                    existing.assignees.as_str(),
                ],
            )
            .map_err(|err| TikError::Index(format!("fts delete: {err}")))?;
        }
        tx.execute(
            "INSERT INTO tickets_fts (
                rowid, title, summary, description, notes, tags, assignees
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                rowid,
                ticket.title.as_str(),
                ticket.summary.as_str(),
                ticket.description.as_str(),
                notes,
                tags.as_str(),
                assignees.as_str(),
            ],
        )
        .map_err(|err| TikError::Index(format!("fts insert: {err}")))?;

        tx.commit()
            .map_err(|err| TikError::Index(format!("commit index: {err}")))?;

        tikfs::remove_file_safe(&self.index_dir().join("tickets.jsonl"))?;
        self.update_metadata(MetadataUpdate::Ticket)?;
        Ok(())
    }

    pub fn record_milestone_write(&self) -> Result<()> {
        self.update_metadata(MetadataUpdate::Milestone)
    }

    pub fn status(&self) -> Result<IndexStatus> {
        let index_path = self.index_path();
        let fts_path = index_path.display().to_string();
        let jsonl_path = self.index_dir().join("tickets.jsonl");
        let metadata_path = self.metadata_path();
        let metadata = self.read_metadata()?;
        let index_present = index_path.is_file();
        let stale = match &metadata {
            Some(meta) => {
                meta.indexed_at < meta.last_ticket_write
                    || meta.indexed_at < meta.last_milestone_write
            }
            None => true,
        };

        Ok(IndexStatus {
            index_present,
            fts_path,
            tickets_jsonl_path: jsonl_path.display().to_string(),
            metadata_path: metadata_path.display().to_string(),
            indexed_at: metadata.as_ref().map(|meta| meta.indexed_at.clone()),
            ticket_count: metadata.as_ref().map(|meta| meta.ticket_count),
            last_ticket_write: metadata.as_ref().map(|meta| meta.last_ticket_write.clone()),
            last_milestone_write: metadata
                .as_ref()
                .map(|meta| meta.last_milestone_write.clone()),
            stale: !index_present || stale,
        })
    }

    #[allow(dead_code)]
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<String>> {
        self.search_page(query, 0, None)
    }

    pub fn search_page(
        &self,
        query: &SearchQuery,
        offset: usize,
        limit: Option<usize>,
    ) -> Result<Vec<String>> {
        let index_path = self.index_path();
        if !index_path.is_file() {
            return Err(TikError::Index("index missing".to_string()));
        }
        let conn = Connection::open(&index_path)
            .map_err(|err| TikError::Index(format!("open index: {err}")))?;

        let (sql, params) = build_search_sql(query, offset, limit);
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|err| TikError::Index(format!("prepare search: {err}")))?;

        let rows = stmt
            .query_map(params_from_iter(params), |row| row.get::<_, String>(0))
            .map_err(|err| TikError::Index(format!("search query: {err}")))?;

        let mut ids = Vec::new();
        for row in rows {
            ids.push(row.map_err(|err| TikError::Index(format!("search row: {err}")))?);
        }
        Ok(ids)
    }

    pub fn list_ids(
        &self,
        status: Option<&str>,
        offset: usize,
        limit: Option<usize>,
    ) -> Result<Vec<String>> {
        let index_path = self.index_path();
        if !index_path.is_file() {
            return Err(TikError::Index("index missing".to_string()));
        }
        let conn = Connection::open(&index_path)
            .map_err(|err| TikError::Index(format!("open index: {err}")))?;

        let mut sql = "SELECT id FROM tickets".to_string();
        let mut params: Vec<SqlValue> = Vec::new();

        if let Some(status) = status {
            sql.push_str(" WHERE status = ?");
            params.push(SqlValue::Text(status.to_string()));
        }

        sql.push_str(" ORDER BY id");

        if let Some(limit) = limit {
            sql.push_str(" LIMIT ?");
            params.push(SqlValue::Integer(limit as i64));
        } else if offset > 0 {
            sql.push_str(" LIMIT ?");
            params.push(SqlValue::Integer(-1));
        }

        if offset > 0 {
            sql.push_str(" OFFSET ?");
            params.push(SqlValue::Integer(offset as i64));
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|err| TikError::Index(format!("prepare list: {err}")))?;

        let rows = stmt
            .query_map(params_from_iter(params), |row| row.get::<_, String>(0))
            .map_err(|err| TikError::Index(format!("list query: {err}")))?;

        let mut ids = Vec::new();
        for row in rows {
            ids.push(row.map_err(|err| TikError::Index(format!("list row: {err}")))?);
        }
        Ok(ids)
    }

    fn index_dir(&self) -> PathBuf {
        self.data_root.join("index")
    }

    fn index_path(&self) -> PathBuf {
        self.index_dir().join("fts.sqlite")
    }

    fn metadata_path(&self) -> PathBuf {
        self.index_dir().join("status.json")
    }

    fn read_metadata(&self) -> Result<Option<IndexMetadata>> {
        let path = self.metadata_path();
        if !path.is_file() {
            return Ok(None);
        }
        let raw = tikfs::read_to_string(&path)?;
        let metadata: IndexMetadata = serde_json::from_str(&raw)
            .map_err(|err| TikError::Index(format!("invalid index metadata: {err}")))?;
        Ok(Some(metadata))
    }

    fn write_metadata(&self, metadata: &IndexMetadata) -> Result<()> {
        let raw = serde_json::to_string_pretty(metadata)
            .map_err(|err| TikError::Index(format!("serialize index metadata: {err}")))?;
        tikfs::write_string_atomic(&self.metadata_path(), &raw)?;
        Ok(())
    }

    fn update_metadata(&self, update: MetadataUpdate) -> Result<()> {
        let index_path = self.index_path();
        if !index_path.is_file() {
            return Err(TikError::Index("index missing".to_string()));
        }
        let now = timeutil::now_rfc3339()?;
        let ticket_count = self.ticket_count(&index_path)?;
        let mut metadata = match self.read_metadata()? {
            Some(metadata) => metadata,
            None => IndexMetadata {
                schema_version: "1.0".to_string(),
                indexed_at: now.clone(),
                ticket_count,
                last_ticket_write: now.clone(),
                last_milestone_write: now.clone(),
            },
        };
        metadata.indexed_at = now.clone();
        metadata.ticket_count = ticket_count;
        match update {
            MetadataUpdate::Ticket => metadata.last_ticket_write = now,
            MetadataUpdate::Milestone => metadata.last_milestone_write = now,
        }
        self.write_metadata(&metadata)
    }

    fn ticket_count(&self, index_path: &Path) -> Result<usize> {
        let conn = Connection::open(index_path)
            .map_err(|err| TikError::Index(format!("open index: {err}")))?;
        conn.query_row("SELECT COUNT(*) FROM tickets", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| count as usize)
        .map_err(|err| TikError::Index(format!("count tickets: {err}")))
    }
}

enum MetadataUpdate {
    Ticket,
    Milestone,
}

struct ExistingIndexRow {
    rowid: i64,
    title: String,
    summary: String,
    description: String,
    notes: String,
    tags: String,
    assignees: String,
}

#[derive(Debug, Serialize)]
struct IndexTicket<'a> {
    id: &'a str,
    title: &'a str,
    summary: &'a str,
    description: &'a str,
    status: &'a str,
    kind: &'a str,
    priority: &'a str,
    severity: &'a str,
    assignees: &'a [String],
    milestone_id: Option<&'a str>,
    tags: &'a [String],
    created_at: &'a str,
    updated_at: &'a str,
    closed_at: Option<&'a str>,
    due_at: Option<&'a str>,
    notes: &'a str,
}

impl<'a> IndexTicket<'a> {
    fn from_ticket(ticket: &'a Ticket, notes: &'a str) -> IndexTicket<'a> {
        IndexTicket {
            id: ticket.id.as_str(),
            title: ticket.title.as_str(),
            summary: ticket.summary.as_str(),
            description: ticket.description.as_str(),
            status: ticket.status.as_str(),
            kind: ticket.kind.as_str(),
            priority: ticket.priority.as_str(),
            severity: ticket.severity.as_str(),
            assignees: &ticket.assignees,
            milestone_id: ticket.milestone_id.as_ref().map(|id| id.as_str()),
            tags: &ticket.tags,
            created_at: ticket.created_at.as_str(),
            updated_at: ticket.updated_at.as_str(),
            closed_at: ticket.closed_at.as_deref(),
            due_at: ticket.due_at.as_deref(),
            notes,
        }
    }
}

fn normalize_list(values: &[String]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let mut normalized: Vec<String> = values
        .iter()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect();
    normalized.sort();
    normalized.dedup();
    format!("\t{}\t", normalized.join("\t"))
}

fn build_search_sql(
    query: &SearchQuery,
    offset: usize,
    limit: Option<usize>,
) -> (String, Vec<String>) {
    let mut conditions = Vec::new();
    let mut params = Vec::new();

    let use_fts = !query.fts_query.is_empty();
    if use_fts {
        conditions.push("tickets_fts MATCH ?".to_string());
        params.push(query.fts_query.clone());
    }

    let filters = &query.filters;
    push_in_clause("status", &filters.status, &mut conditions, &mut params);
    push_in_clause("type", &filters.types, &mut conditions, &mut params);
    push_in_clause(
        "priority",
        &filters.priorities,
        &mut conditions,
        &mut params,
    );
    push_in_clause(
        "severity",
        &filters.severities,
        &mut conditions,
        &mut params,
    );
    push_in_clause(
        "milestone_id",
        &filters.milestone_ids,
        &mut conditions,
        &mut params,
    );

    for tag in &filters.tags {
        conditions.push("instr(tags, ?) > 0".to_string());
        params.push(format!("\t{}\t", tag.to_lowercase()));
    }

    for assignee in &filters.assignees {
        conditions.push("instr(assignees, ?) > 0".to_string());
        params.push(format!("\t{}\t", assignee.to_lowercase()));
    }

    apply_date_filter("created_at", &filters.created, &mut conditions, &mut params);
    apply_date_filter("updated_at", &filters.updated, &mut conditions, &mut params);
    apply_date_filter("closed_at", &filters.closed, &mut conditions, &mut params);
    apply_date_filter("due_at", &filters.due, &mut conditions, &mut params);

    let base = if use_fts {
        "SELECT tickets.id FROM tickets JOIN tickets_fts ON tickets_fts.rowid = tickets.rowid"
            .to_string()
    } else {
        "SELECT id FROM tickets".to_string()
    };
    let mut sql = base;
    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }
    if use_fts {
        sql.push_str(" ORDER BY bm25(tickets_fts)");
    } else {
        sql.push_str(" ORDER BY updated_at DESC, id");
    }

    if let Some(limit) = limit {
        sql.push_str(" LIMIT ?");
        params.push(limit.to_string());
    } else if offset > 0 {
        sql.push_str(" LIMIT ?");
        params.push("-1".to_string());
    }

    if offset > 0 {
        sql.push_str(" OFFSET ?");
        params.push(offset.to_string());
    }
    (sql, params)
}

fn push_in_clause(
    column: &str,
    values: &[String],
    conditions: &mut Vec<String>,
    params: &mut Vec<String>,
) {
    if values.is_empty() {
        return;
    }
    let placeholders = vec!["?"; values.len()].join(", ");
    conditions.push(format!("{column} IN ({placeholders})"));
    for value in values {
        params.push(value.clone());
    }
}

fn apply_date_filter(
    column: &str,
    filter: &DateFilter,
    conditions: &mut Vec<String>,
    params: &mut Vec<String>,
) {
    if let Some(prefix) = &filter.prefix {
        conditions.push(format!("{column} LIKE ?"));
        params.push(format!("{prefix}%"));
    }
    if let Some(DateBound { value, inclusive }) = &filter.after {
        let op = if *inclusive { ">=" } else { ">" };
        conditions.push(format!("{column} {op} ?"));
        params.push(value.clone());
    }
    if let Some(DateBound { value, inclusive }) = &filter.before {
        let op = if *inclusive { "<=" } else { "<" };
        conditions.push(format!("{column} {op} ?"));
        params.push(value.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ticket::NewTicket;
    use tempfile::tempdir;

    #[test]
    fn index_status_reports_missing() {
        let dir = tempdir().unwrap();
        let data_root = dir.path().join(".tik");
        tikfs::ensure_dir(&data_root).unwrap();

        let store = IndexStore::new(data_root);
        let status = store.status().unwrap();
        assert!(!status.index_present);
        assert!(status.stale);
    }

    #[test]
    fn rebuild_and_search_index() {
        let dir = tempdir().unwrap();
        let data_root = dir.path().join(".tik");
        tikfs::ensure_dir(&data_root).unwrap();

        let ticket = Ticket::new(
            NewTicket {
                title: "Search Title".to_string(),
                summary: Some("Summary".to_string()),
                description: Some("Description".to_string()),
                tags: vec!["mvp".to_string()],
            },
            "2026-01-01T00:00:00Z",
        );
        let mut notes = HashMap::new();
        notes.insert(ticket.id.as_str().to_string(), "Notes text".to_string());

        let store = IndexStore::new(data_root);
        let summary = store
            .rebuild(std::slice::from_ref(&ticket), &notes)
            .unwrap();
        assert_eq!(summary.ticket_count, 1);

        let query = SearchQuery::parse("status:open mvp").unwrap();
        let results = store.search(&query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], ticket.id.as_str());
    }

    #[test]
    fn upsert_ticket_updates_index() {
        let dir = tempdir().unwrap();
        let data_root = dir.path().join(".tik");
        tikfs::ensure_dir(&data_root).unwrap();

        let mut ticket = Ticket::new(
            NewTicket {
                title: "Original".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        ticket.id = crate::domain::ids::TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        let mut notes = HashMap::new();
        notes.insert(ticket.id.as_str().to_string(), "Initial notes".to_string());

        let store = IndexStore::new(data_root);
        store.rebuild(&[ticket.clone()], &notes).unwrap();

        let mut updated = ticket.clone();
        updated.title = "Updated Title".to_string();
        updated.updated_at = "2026-01-02T00:00:00Z".to_string();
        store.upsert_ticket(&updated, "Updated notes").unwrap();

        let query = SearchQuery::parse("Updated").unwrap();
        let results = store.search(&query).unwrap();
        assert_eq!(results, vec![updated.id.as_str().to_string()]);
    }

    #[test]
    fn record_milestone_write_updates_metadata() {
        let dir = tempdir().unwrap();
        let data_root = dir.path().join(".tik");
        tikfs::ensure_dir(&data_root).unwrap();

        let ticket = Ticket::new(
            NewTicket {
                title: "Meta".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        let mut notes = HashMap::new();
        notes.insert(ticket.id.as_str().to_string(), String::new());

        let store = IndexStore::new(data_root);
        store.rebuild(&[ticket], &notes).unwrap();
        store.record_milestone_write().unwrap();

        let status = store.status().unwrap();
        assert!(status.last_milestone_write.is_some());
        assert!(!status.stale);
    }

    #[test]
    fn list_ids_respects_offset_and_limit() {
        let dir = tempdir().unwrap();
        let data_root = dir.path().join(".tik");
        tikfs::ensure_dir(&data_root).unwrap();

        let mut ticket_a = Ticket::new(
            NewTicket {
                title: "Alpha".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        ticket_a.id = crate::domain::ids::TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        let mut ticket_b = Ticket::new(
            NewTicket {
                title: "Beta".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        ticket_b.id = crate::domain::ids::TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAW").unwrap();

        let mut notes = HashMap::new();
        notes.insert(ticket_a.id.as_str().to_string(), String::new());
        notes.insert(ticket_b.id.as_str().to_string(), String::new());

        let store = IndexStore::new(data_root);
        store
            .rebuild(&[ticket_a.clone(), ticket_b.clone()], &notes)
            .unwrap();

        let first = store.list_ids(None, 0, Some(1)).unwrap();
        let second = store.list_ids(None, 1, Some(1)).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_ne!(first[0], second[0]);
    }
}
