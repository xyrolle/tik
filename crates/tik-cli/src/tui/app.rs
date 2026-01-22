//! TUI application state.

use std::collections::HashSet;
use std::path::PathBuf;

use ratatui::Frame;
use tik_core::{Repo, Result, Ticket, TicketId};

use super::input::ticket_matches_query;
use super::render::draw;
use super::state::{
    DetailsTab, DiffLine, DiffSnapshot, EditorTarget, GraphState, Mode, Panel, PendingEditor,
    PendingKey, TicketDetails, TicketFilter, TicketSummary,
};

/// Main TUI application state.
pub struct App {
    pub root: PathBuf,
    pub repo: Option<Repo>,
    pub tickets: Vec<Ticket>,
    pub selected: usize,
    pub filter: TicketFilter,
    pub search_query: String,
    pub mode: Mode,
    pub status: String,
    pub no_color: bool,
    pub focused_panel: Panel,
    pub details_tab: DetailsTab,
    pub ticket_details: TicketDetails,
    pub pending_key: PendingKey,
    pub pending_editor: PendingEditor,
    /// Selected ticket IDs for multi-select operations.
    pub selected_ids: HashSet<String>,
    /// Whether selection mode is active.
    pub selection_mode: bool,
    page_size: usize,
}

impl App {
    /// Create a new App instance.
    pub fn new(root: PathBuf, repo: Option<Repo>, no_color: bool) -> Result<Self> {
        let mut app = Self {
            root,
            repo,
            tickets: Vec::new(),
            selected: 0,
            filter: TicketFilter::All,
            search_query: String::new(),
            mode: Mode::Normal,
            status: String::new(),
            no_color,
            focused_panel: Panel::TicketList,
            details_tab: DetailsTab::Info,
            ticket_details: TicketDetails::default(),
            pending_key: PendingKey::None,
            pending_editor: PendingEditor::default(),
            selected_ids: HashSet::new(),
            selection_mode: false,
            page_size: 20,
        };
        app.refresh(None)?;
        Ok(app)
    }

    /// Draw the UI.
    pub fn draw(&self, frame: &mut Frame) {
        draw(frame, self);
    }

    /// Refresh ticket list from repo.
    pub fn refresh(&mut self, select_id: Option<&str>) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.tickets.clear();
            return Ok(());
        };
        let mut tickets = repo.list_tickets(self.filter.status())?;
        if !self.search_query.trim().is_empty() {
            let query = self.search_query.trim();
            tickets.retain(|ticket| ticket_matches_query(ticket, query));
        }
        self.tickets = tickets;
        if let Some(id) = select_id {
            if let Some(pos) = self
                .tickets
                .iter()
                .position(|ticket| ticket.id.as_str() == id)
            {
                self.selected = pos;
                self.load_ticket_details()?;
                return Ok(());
            }
        }
        if self.selected >= self.tickets.len() {
            self.selected = self.tickets.len().saturating_sub(1);
        }
        self.load_ticket_details()?;
        Ok(())
    }

    /// Load details for currently selected ticket.
    pub fn load_ticket_details(&mut self) -> Result<()> {
        let Some(ticket) = self.tickets.get(self.selected) else {
            self.ticket_details.clear();
            return Ok(());
        };

        let ticket_id = ticket.id.as_str().to_string();

        // Only reload if ticket changed
        if self.ticket_details.ticket_id.as_deref() == Some(&ticket_id) {
            return Ok(());
        }

        let Some(repo) = &self.repo else {
            self.ticket_details.clear();
            return Ok(());
        };

        let tid = TicketId::parse(&ticket_id)?;

        // Load notes
        let notes = repo.read_ticket_notes(&tid).ok();

        // Load events
        let events = repo.read_events(&tid).unwrap_or_default();

        let relation_targets = ticket
            .relations
            .iter()
            .map(|relation| {
                repo.load_ticket(&relation.id).ok().map(|target| TicketSummary {
                    id: target.id.as_str().to_string(),
                    title: target.title.clone(),
                    status: target.status.clone(),
                })
            })
            .collect();

        let milestone = match ticket.milestone_id.as_ref() {
            Some(milestone_id) => repo.load_milestone(milestone_id).ok(),
            None => None,
        };

        self.ticket_details = TicketDetails {
            ticket_id: Some(ticket_id),
            notes,
            events,
            scroll_offset: 0,
            relation_selected: 0,
            relation_targets,
            milestone,
            diff_state: Default::default(),
        };

        Ok(())
    }

    /// Get currently selected ticket ID.
    pub fn current_ticket_id(&self) -> Option<String> {
        self.tickets
            .get(self.selected)
            .map(|ticket| ticket.id.as_str().to_string())
    }

    /// Move selection down.
    pub fn move_down(&mut self) {
        match self.focused_panel {
            Panel::TicketList => {
                if self.tickets.is_empty() {
                    return;
                }
                let new_selected = if self.selected + 1 < self.tickets.len() {
                    self.selected + 1
                } else {
                    0
                };
                if new_selected != self.selected {
                    self.selected = new_selected;
                    let _ = self.load_ticket_details();
                }
            }
            Panel::Details => {
                // In Relations tab, navigate through relations
                if self.details_tab == DetailsTab::Relations {
                    if let Some(ticket) = self.tickets.get(self.selected) {
                        let max_rel = ticket.relations.len().saturating_sub(1);
                        if self.ticket_details.relation_selected < max_rel {
                            self.ticket_details.relation_selected += 1;
                        }
                    }
                } else {
                    self.ticket_details.scroll_offset += 1;
                }
            }
        }
    }

    /// Move selection up.
    pub fn move_up(&mut self) {
        match self.focused_panel {
            Panel::TicketList => {
                if self.tickets.is_empty() {
                    return;
                }
                let new_selected = if self.selected > 0 {
                    self.selected - 1
                } else {
                    self.tickets.len() - 1
                };
                if new_selected != self.selected {
                    self.selected = new_selected;
                    let _ = self.load_ticket_details();
                }
            }
            Panel::Details => {
                // In Relations tab, navigate through relations
                if self.details_tab == DetailsTab::Relations {
                    if self.ticket_details.relation_selected > 0 {
                        self.ticket_details.relation_selected -= 1;
                    }
                } else {
                    self.ticket_details.scroll_offset =
                        self.ticket_details.scroll_offset.saturating_sub(1);
                }
            }
        }
    }

    /// Go to top of list.
    pub fn go_to_top(&mut self) {
        match self.focused_panel {
            Panel::TicketList => {
                if !self.tickets.is_empty() {
                    self.selected = 0;
                    let _ = self.load_ticket_details();
                }
            }
            Panel::Details => {
                self.ticket_details.scroll_offset = 0;
            }
        }
    }

    /// Go to bottom of list.
    pub fn go_to_bottom(&mut self) {
        match self.focused_panel {
            Panel::TicketList => {
                if !self.tickets.is_empty() {
                    self.selected = self.tickets.len() - 1;
                    let _ = self.load_ticket_details();
                }
            }
            Panel::Details => {
                // Scroll to a large number (will be clamped by content length)
                self.ticket_details.scroll_offset = usize::MAX / 2;
            }
        }
    }

    /// Move half page down.
    pub fn half_page_down(&mut self) {
        let half = self.page_size / 2;
        match self.focused_panel {
            Panel::TicketList => {
                let new_pos = (self.selected + half).min(self.tickets.len().saturating_sub(1));
                if new_pos != self.selected {
                    self.selected = new_pos;
                    let _ = self.load_ticket_details();
                }
            }
            Panel::Details => {
                self.ticket_details.scroll_offset += half;
            }
        }
    }

    /// Move half page up.
    pub fn half_page_up(&mut self) {
        let half = self.page_size / 2;
        match self.focused_panel {
            Panel::TicketList => {
                let new_pos = self.selected.saturating_sub(half);
                if new_pos != self.selected {
                    self.selected = new_pos;
                    let _ = self.load_ticket_details();
                }
            }
            Panel::Details => {
                self.ticket_details.scroll_offset =
                    self.ticket_details.scroll_offset.saturating_sub(half);
            }
        }
    }

    /// Request to edit ticket.json in $EDITOR.
    pub fn request_edit_ticket(&mut self) {
        if let Some(ticket) = self.tickets.get(self.selected) {
            let id = ticket.id.as_str().to_string();
            self.request_edit_ticket_id(&id);
        } else {
            self.status = "no ticket selected".to_string();
        }
    }

    /// Request to edit ticket.json for a specific ticket.
    pub fn request_edit_ticket_id(&mut self, id: &str) {
        if id.is_empty() {
            self.status = "no ticket selected".to_string();
            return;
        }
        self.pending_editor.target = Some(EditorTarget::Ticket(id.to_string()));
    }

    /// Request to edit notes.md in $EDITOR.
    pub fn request_edit_notes(&mut self) {
        if let Some(ticket) = self.tickets.get(self.selected) {
            let id = ticket.id.as_str().to_string();
            self.request_edit_notes_id(&id);
        } else {
            self.status = "no ticket selected".to_string();
        }
    }

    /// Request to edit notes.md for a specific ticket.
    pub fn request_edit_notes_id(&mut self, id: &str) {
        if id.is_empty() {
            self.status = "no ticket selected".to_string();
            return;
        }
        self.pending_editor.target = Some(EditorTarget::Notes(id.to_string()));
    }

    /// Take the pending editor target (if any).
    pub fn take_pending_editor(&mut self) -> Option<EditorTarget> {
        self.pending_editor.target.take()
    }

    /// Get the file path for an editor target.
    pub fn editor_target_path(&self, target: &EditorTarget) -> Option<PathBuf> {
        let repo = self.repo.as_ref()?;
        let id = match target {
            EditorTarget::Ticket(id) | EditorTarget::Notes(id) => id.as_str(),
        };
        let ticket_id = TicketId::parse(id).ok()?;
        let path = match target {
            EditorTarget::Ticket(_) => repo.ticket_path(&ticket_id),
            EditorTarget::Notes(_) => repo.notes_path(&ticket_id),
        };
        Some(path)
    }

    /// Reload after editor closes. Validates and applies changes for ticket edits.
    pub fn after_editor(&mut self, target: &EditorTarget) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };

        match target {
            EditorTarget::Ticket(id) => {
                let ticket_id = TicketId::parse(id)?;
                let ticket_path = repo.ticket_path(&ticket_id);

                // Read the edited content
                let content = std::fs::read_to_string(&ticket_path)
                    .map_err(|err| tik_core::TikError::io("read edited ticket", err))?;

                // Validate by parsing
                let ticket: tik_core::Ticket = serde_json::from_str(&content).map_err(|err| {
                    tik_core::TikError::Schema(format!("invalid ticket JSON: {err}"))
                })?;
                if ticket.id != ticket_id {
                    return Err(tik_core::TikError::Schema(
                        "ticket id mismatch (id is immutable)".to_string(),
                    ));
                }

                // Apply the edit through repo to record the event
                let actor = std::env::var("TIK_ACTOR")
                    .or_else(|_| std::env::var("USER"))
                    .unwrap_or_else(|_| "unknown".to_string());

                repo.apply_edit(&ticket_id, &content, &actor, Some("edited in $EDITOR"))?;

                self.refresh(Some(id))?;
                // Force reload of ticket details
                self.ticket_details.ticket_id = None;
                self.load_ticket_details()?;
                self.status = format!("saved {id}");
            }
            EditorTarget::Notes(id) => {
                // Notes are saved directly to notes.md - just refresh
                self.refresh(Some(id))?;
                self.ticket_details.ticket_id = None;
                self.load_ticket_details()?;
                self.status = format!("saved notes for {id}");
            }
        }

        Ok(())
    }

    /// Toggle selection for the current ticket.
    pub fn toggle_selection(&mut self) {
        if let Some(ticket) = self.tickets.get(self.selected) {
            let id = ticket.id.as_str().to_string();
            if self.selected_ids.contains(&id) {
                self.selected_ids.remove(&id);
            } else {
                self.selected_ids.insert(id);
            }
        }
    }

    /// Exit selection mode.
    pub fn exit_selection_mode(&mut self) {
        self.selection_mode = false;
        self.selected_ids.clear();
        self.status = "exited selection mode".to_string();
    }

    /// Select all visible tickets.
    pub fn select_all_visible(&mut self) {
        for ticket in &self.tickets {
            self.selected_ids.insert(ticket.id.as_str().to_string());
        }
        self.selection_mode = true;
        self.status = format!("{} tickets selected", self.selected_ids.len());
    }

    /// Deselect all tickets.
    pub fn deselect_all(&mut self) {
        self.selected_ids.clear();
        self.status = "selection cleared".to_string();
    }

    /// Check if a ticket is selected.
    pub fn is_selected(&self, ticket_id: &str) -> bool {
        self.selected_ids.contains(ticket_id)
    }

    /// Get the number of selected tickets.
    pub fn selection_count(&self) -> usize {
        self.selected_ids.len()
    }

    /// Load diff snapshots for the current ticket.
    pub fn load_diff_snapshots(&mut self) -> Result<()> {
        let Some(ticket) = self.tickets.get(self.selected) else {
            self.ticket_details.diff_state.clear();
            return Ok(());
        };

        let Some(repo) = &self.repo else {
            self.ticket_details.diff_state.clear();
            return Ok(());
        };

        let tid = TicketId::parse(ticket.id.as_str())?;
        let events = repo.read_events(&tid)?;

        // Find snapshot events (created and ticket_edited have full snapshots)
        let mut snapshots: Vec<DiffSnapshot> = Vec::new();

        for event in &events {
            // Check for "ticket" key which contains the full ticket JSON snapshot
            // Events like "created", "ticket_edited", and "imported" store snapshots under "ticket"
            if event.data.get("ticket").is_some() {
                let content = if let Some(snapshot) = event.data.get("ticket") {
                    serde_json::to_string_pretty(snapshot).unwrap_or_default()
                } else {
                    String::new()
                };

                if !content.is_empty() && content != "{}" && content != "null" {
                    snapshots.push(DiffSnapshot {
                        timestamp: event.ts.clone(),
                        actor: event.actor.clone(),
                        content,
                    });
                }
            }
        }

        // If we don't have snapshots from events, create one from current state
        if snapshots.is_empty() {
            let current_content = serde_json::to_string_pretty(ticket).unwrap_or_default();
            snapshots.push(DiffSnapshot {
                timestamp: ticket.updated_at.clone(),
                actor: "current".to_string(),
                content: current_content,
            });
        }

        // Set up diff state
        self.ticket_details.diff_state.snapshots = snapshots;
        self.ticket_details.diff_state.before_idx = 0;
        self.ticket_details.diff_state.after_idx = self
            .ticket_details
            .diff_state
            .snapshots
            .len()
            .saturating_sub(1);

        self.compute_diff()?;
        Ok(())
    }

    /// Compute the diff between selected versions.
    pub fn compute_diff(&mut self) -> Result<()> {
        use similar::{ChangeTag, TextDiff};

        let diff_state = &mut self.ticket_details.diff_state;

        if diff_state.snapshots.len() < 2 {
            diff_state.diff_lines.clear();
            return Ok(());
        }

        let before = &diff_state.snapshots[diff_state.before_idx];
        let after = &diff_state.snapshots[diff_state.after_idx];

        let text_diff = TextDiff::from_lines(&before.content, &after.content);

        let mut lines = Vec::new();

        // Add header
        lines.push(DiffLine::Header(format!(
            "--- {} ({})",
            before.timestamp, before.actor
        )));
        lines.push(DiffLine::Header(format!(
            "+++ {} ({})",
            after.timestamp, after.actor
        )));
        lines.push(DiffLine::Context(String::new()));

        for change in text_diff.iter_all_changes() {
            let text = change.value().trim_end().to_string();
            match change.tag() {
                ChangeTag::Delete => {
                    lines.push(DiffLine::Removed(text));
                }
                ChangeTag::Insert => {
                    lines.push(DiffLine::Added(text));
                }
                ChangeTag::Equal => {
                    lines.push(DiffLine::Context(text));
                }
            }
        }

        diff_state.diff_lines = lines;
        Ok(())
    }

    /// Open the dependency graph view.
    pub fn open_graph_view(&mut self) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };

        // Get current ticket ID as root if available
        let root_id = self.current_ticket_id();

        // Build the graph
        let graph = repo.graph()?;

        if graph.nodes.is_empty() {
            self.status = "no tickets with relations found".to_string();
            return Ok(());
        }

        let state = GraphState::new(graph, root_id);
        self.mode = Mode::GraphView(state);
        self.status = format!(
            "graph: {} nodes, {} edges",
            self.tickets.len(),
            0 // We don't have direct access to edge count here
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tik_core::{Event, NewTicket, TicketId};

    #[test]
    fn refresh_filters_by_search_query() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0-test").unwrap();
        repo.create_ticket(
            NewTicket {
                title: "Alpha".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "tester",
        )
        .unwrap();
        repo.create_ticket(
            NewTicket {
                title: "Beta".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "tester",
        )
        .unwrap();

        let mut app = App::new(dir.path().to_path_buf(), Some(repo), true).unwrap();
        app.search_query = "Alpha".to_string();
        app.refresh(None).unwrap();
        assert_eq!(app.tickets.len(), 1);
        assert_eq!(app.tickets[0].title, "Alpha");
    }

    #[test]
    fn list_navigation_wraps() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0-test").unwrap();
        repo.create_ticket(
            NewTicket {
                title: "First".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "tester",
        )
        .unwrap();
        repo.create_ticket(
            NewTicket {
                title: "Second".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "tester",
        )
        .unwrap();

        let mut app = App::new(dir.path().to_path_buf(), Some(repo), true).unwrap();
        app.focused_panel = Panel::TicketList;
        app.selected = 0;
        app.move_up();
        assert_eq!(app.selected, 1);
        app.move_down();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn details_scroll_moves_and_bounds() {
        let dir = tempdir().unwrap();
        let mut app = App::new(dir.path().to_path_buf(), None, true).unwrap();
        app.focused_panel = Panel::Details;
        app.ticket_details.scroll_offset = 1;
        app.move_down();
        assert_eq!(app.ticket_details.scroll_offset, 2);
        app.move_up();
        assert_eq!(app.ticket_details.scroll_offset, 1);
        app.go_to_bottom();
        assert!(app.ticket_details.scroll_offset > 1);
        app.go_to_top();
        assert_eq!(app.ticket_details.scroll_offset, 0);
    }

    #[test]
    fn load_ticket_details_clears_when_empty() {
        let dir = tempdir().unwrap();
        let mut app = App::new(dir.path().to_path_buf(), None, true).unwrap();
        app.ticket_details.ticket_id = Some("T-TEST".to_string());
        app.ticket_details.notes = Some("notes".to_string());
        app.ticket_details.events = vec![Event::note("actor", "ts", "text")];
        app.load_ticket_details().unwrap();
        assert!(app.ticket_details.ticket_id.is_none());
        assert!(app.ticket_details.notes.is_none());
        assert!(app.ticket_details.events.is_empty());
    }

    #[test]
    fn load_ticket_details_skips_when_cached() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0-test").unwrap();
        repo.create_ticket(
            NewTicket {
                title: "Cached".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "tester",
        )
        .unwrap();
        let mut app = App::new(dir.path().to_path_buf(), Some(repo), true).unwrap();
        let current_id = app.current_ticket_id().unwrap();
        app.ticket_details.ticket_id = Some(current_id);
        app.ticket_details.notes = Some("cached".to_string());
        app.load_ticket_details().unwrap();
        assert_eq!(app.ticket_details.notes.as_deref(), Some("cached"));
    }

    #[test]
    fn after_editor_rejects_id_change() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0-test").unwrap();
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Immutable".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "tester",
            )
            .unwrap();
        let ticket_id = ticket.id.clone();
        let mut app = App::new(dir.path().to_path_buf(), Some(repo), true).unwrap();
        let path = app
            .repo
            .as_ref()
            .unwrap()
            .ticket_path(&ticket_id);

        let mut edited = ticket.clone();
        edited.id = TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAZ").unwrap();
        let raw = serde_json::to_string_pretty(&edited).unwrap();
        std::fs::write(&path, raw).unwrap();

        let result = app.after_editor(&EditorTarget::Ticket(ticket_id.as_str().to_string()));
        assert!(result.is_err());
    }
}
