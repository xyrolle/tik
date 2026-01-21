//! TUI application state.

use std::collections::HashSet;
use std::path::PathBuf;

use ratatui::Frame;
use tik_core::{Repo, Result, Ticket, TicketId};

use super::input::ticket_matches_query;
use super::render::draw;
use super::state::{
    DetailsTab, EditorTarget, Mode, Panel, PendingEditor, PendingKey, TicketDetails, TicketFilter,
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

        self.ticket_details = TicketDetails {
            ticket_id: Some(ticket_id),
            notes,
            events,
            scroll_offset: 0,
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
                self.ticket_details.scroll_offset += 1;
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
                self.ticket_details.scroll_offset =
                    self.ticket_details.scroll_offset.saturating_sub(1);
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
                let _ticket: tik_core::Ticket = serde_json::from_str(&content).map_err(|err| {
                    tik_core::TikError::Schema(format!("invalid ticket JSON: {err}"))
                })?;

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
}
