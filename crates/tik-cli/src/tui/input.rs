//! TUI input handling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tik_core::{MilestoneId, Priority, Result, Severity, TicketId, TicketStatus, TikError};
#[allow(unused_imports)]
use tik_core::TicketStatus as _;

use super::app::App;
use super::state::{
    Action, ActionItem, ActionState, ConfirmAction, ConfirmState, DetailsTab, InitFocus, InitState,
    InputKind, InputState, MilestoneMenuState, Mode, Panel, PendingKey, RelationEditState,
    RelationStep, SearchState, SelectKind, SelectState, TagEditState, TicketFilter, TicketSummary,
};

impl App {
    /// Handle a key event. Returns true if the app should quit.
    pub fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        // Clear pending key after timeout (handled elsewhere, but clear on any input)
        let pending = self.pending_key;
        self.pending_key = PendingKey::None;

        match &self.mode {
            Mode::Input(_) => return self.handle_input_mode(key),
            Mode::Confirm(_) => return self.handle_confirm_mode(key),
            Mode::Init(_) => return self.handle_init_mode(key),
            Mode::Action(_) => return self.handle_action_mode(key),
            Mode::Search(_) => return self.handle_search_mode(key),
            Mode::Select(_) => return self.handle_select_mode(key),
            Mode::Help => return self.handle_help_mode(key),
            Mode::MilestoneMenu(_) => return self.handle_milestone_mode(key),
            Mode::TagEdit(_) => return self.handle_tag_edit_mode(key),
            Mode::RelationEdit(_) => return self.handle_relation_edit_mode(key),
            Mode::GraphView(_) => return self.handle_graph_view_mode(key),
            Mode::Normal => {}
        }

        if matches!(key.code, KeyCode::Char('q')) {
            if self.selection_mode {
                self.exit_selection_mode();
                return Ok(false);
            }
            return Ok(true);
        }

        // Handle Esc to exit selection mode
        if matches!(key.code, KeyCode::Esc) && self.selection_mode {
            self.exit_selection_mode();
            return Ok(false);
        }

        if self.repo.is_none() {
            return self.handle_uninitialized_key(key);
        }

        // Handle pending g key for gg sequence
        if pending == PendingKey::G && matches!(key.code, KeyCode::Char('g')) {
            self.go_to_top();
            return Ok(false);
        }

        match key.code {
            // Vim navigation
            KeyCode::Char('j') | KeyCode::Down => self.move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.move_up(),
            KeyCode::Char('g') => {
                self.pending_key = PendingKey::G;
            }
            KeyCode::Char('G') => self.go_to_bottom(),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.half_page_down();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.half_page_up();
            }

            // Panel switching / Tab navigation
            KeyCode::Tab => {
                if self.focused_panel == Panel::Details {
                    // Cycle through detail tabs
                    let prev_tab = self.details_tab;
                    self.details_tab = self.details_tab.next();
                    self.ticket_details.reset_scroll();
                    if self.details_tab == DetailsTab::Diff && prev_tab != DetailsTab::Diff {
                        self.load_diff_snapshots()?;
                    }
                } else {
                    self.focused_panel = self.focused_panel.next();
                    self.ticket_details.reset_scroll();
                }
            }
            KeyCode::BackTab => {
                if self.focused_panel == Panel::Details {
                    // Cycle through detail tabs backwards
                    let prev_tab = self.details_tab;
                    self.details_tab = self.details_tab.prev();
                    self.ticket_details.reset_scroll();
                    if self.details_tab == DetailsTab::Diff && prev_tab != DetailsTab::Diff {
                        self.load_diff_snapshots()?;
                    }
                } else {
                    self.focused_panel = self.focused_panel.next();
                    self.ticket_details.reset_scroll();
                }
            }
            KeyCode::Enter => {
                if self.focused_panel == Panel::TicketList && !self.tickets.is_empty() {
                    self.focused_panel = Panel::Details;
                    self.ticket_details.reset_scroll();
                } else if self.focused_panel == Panel::Details
                    && self.details_tab == DetailsTab::Relations
                {
                    // Jump to selected relation target
                    self.jump_to_selected_relation()?;
                }
            }
            KeyCode::Esc => {
                if self.focused_panel == Panel::Details {
                    self.focused_panel = Panel::TicketList;
                }
            }
            KeyCode::Char('h') => {
                if self.focused_panel == Panel::Details {
                    self.focused_panel = Panel::TicketList;
                }
            }
            KeyCode::Char('l') => {
                if self.focused_panel == Panel::TicketList {
                    self.focused_panel = Panel::Details;
                }
            }

            // Tab switching in details panel ([ ] or Left/Right arrows)
            KeyCode::Char('[') | KeyCode::Left => {
                if self.focused_panel == Panel::Details {
                    let prev_tab = self.details_tab;
                    self.details_tab = self.details_tab.prev();
                    self.ticket_details.reset_scroll();
                    if self.details_tab == DetailsTab::Diff && prev_tab != DetailsTab::Diff {
                        self.load_diff_snapshots()?;
                    }
                }
            }
            KeyCode::Char(']') | KeyCode::Right => {
                if self.focused_panel == Panel::Details {
                    let prev_tab = self.details_tab;
                    self.details_tab = self.details_tab.next();
                    self.ticket_details.reset_scroll();
                    if self.details_tab == DetailsTab::Diff && prev_tab != DetailsTab::Diff {
                        self.load_diff_snapshots()?;
                    }
                }
            }
            KeyCode::Char('1') => {
                self.details_tab = DetailsTab::Info;
                self.ticket_details.reset_scroll();
            }
            KeyCode::Char('2') => {
                self.details_tab = DetailsTab::Notes;
                self.ticket_details.reset_scroll();
            }
            KeyCode::Char('3') => {
                self.details_tab = DetailsTab::History;
                self.ticket_details.reset_scroll();
            }
            KeyCode::Char('4') => {
                self.details_tab = DetailsTab::Relations;
                self.ticket_details.reset_scroll();
            }
            KeyCode::Char('5') => {
                self.details_tab = DetailsTab::Diff;
                self.ticket_details.reset_scroll();
                self.load_diff_snapshots()?;
            }

            // Diff version navigation (in Diff tab)
            KeyCode::Char('<')
                if self.focused_panel == Panel::Details
                    && self.details_tab == DetailsTab::Diff =>
            {
                self.ticket_details.diff_state.prev_after();
                self.compute_diff()?;
            }
            KeyCode::Char('>')
                if self.focused_panel == Panel::Details
                    && self.details_tab == DetailsTab::Diff =>
            {
                self.ticket_details.diff_state.next_after();
                self.compute_diff()?;
            }

            // Relation management (in Relations tab)
            KeyCode::Char('+') => {
                if self.focused_panel == Panel::Details && self.details_tab == DetailsTab::Relations
                {
                    if let Some(ticket) = self.tickets.get(self.selected) {
                        self.open_relation_edit(ticket.id.as_str().to_string())?;
                    } else {
                        self.status = "no ticket selected".to_string();
                    }
                }
            }
            KeyCode::Char('d')
                if self.focused_panel == Panel::Details
                    && self.details_tab == DetailsTab::Relations =>
            {
                self.remove_selected_relation()?;
            }

            // Filter
            KeyCode::Char('f') => {
                let current_id = self.current_ticket_id();
                self.filter = self.filter.next();
                self.refresh(current_id.as_deref())?;
                self.status = format!("filter: {}", self.filter.label());
            }
            KeyCode::Char('F') => {
                self.mode = Mode::Select(SelectState::filter());
            }

            // Refresh
            KeyCode::Char('R') => {
                self.refresh(None)?;
                self.status = "refreshed".to_string();
            }

            // New ticket
            KeyCode::Char('n') => {
                self.mode = Mode::Input(InputState::new_ticket());
            }

            // Add note
            KeyCode::Char('a') => {
                if let Some(ticket) = self.tickets.get(self.selected) {
                    self.mode = Mode::Input(InputState::add_note(ticket.id.as_str().to_string()));
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Close ticket(s)
            KeyCode::Char('c') => {
                if self.selection_mode && !self.selected_ids.is_empty() {
                    let ids = self.selected_ids_sorted();
                    self.mode = Mode::Confirm(ConfirmState {
                        prompt: format!("Close {} selected ticket(s)?", ids.len()),
                        action: ConfirmAction::BulkClose(ids),
                    });
                } else if let Some(ticket) = self.tickets.get(self.selected) {
                    self.request_close(ticket.id.as_str().to_string());
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Reopen ticket(s)
            KeyCode::Char('r') => {
                if self.selection_mode && !self.selected_ids.is_empty() {
                    let ids = self.selected_ids_sorted();
                    self.mode = Mode::Confirm(ConfirmState {
                        prompt: format!("Reopen {} selected ticket(s)?", ids.len()),
                        action: ConfirmAction::BulkReopen(ids),
                    });
                } else if let Some(ticket) = self.tickets.get(self.selected) {
                    self.request_reopen(ticket.id.as_str().to_string());
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Set priority (selection menu)
            KeyCode::Char('p') => {
                let Some(repo) = &self.repo else {
                    self.status = "repo not initialized".to_string();
                    return Ok(false);
                };
                let priorities = repo.config_show()?.ticket_priorities;
                if self.selection_mode && !self.selected_ids.is_empty() {
                    let ids = self.selected_ids_sorted();
                    self.mode = Mode::Select(SelectState::priority_bulk(ids, priorities));
                } else if let Some(ticket) = self.tickets.get(self.selected) {
                    self.mode = Mode::Select(SelectState::priority(
                        ticket.id.as_str().to_string(),
                        priorities,
                    ));
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Set severity (selection menu)
            KeyCode::Char('v') => {
                let Some(repo) = &self.repo else {
                    self.status = "repo not initialized".to_string();
                    return Ok(false);
                };
                let severities = repo.config_show()?.ticket_severities;
                if self.selection_mode && !self.selected_ids.is_empty() {
                    let ids = self.selected_ids_sorted();
                    self.mode = Mode::Select(SelectState::severity_bulk(ids, severities));
                } else if let Some(ticket) = self.tickets.get(self.selected) {
                    self.mode = Mode::Select(SelectState::severity(
                        ticket.id.as_str().to_string(),
                        severities,
                    ));
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Set status (selection menu)
            KeyCode::Char('s') => {
                let Some(repo) = &self.repo else {
                    self.status = "repo not initialized".to_string();
                    return Ok(false);
                };
                let statuses = repo.config_show()?.ticket_statuses;
                if self.selection_mode && !self.selected_ids.is_empty() {
                    let ids = self.selected_ids_sorted();
                    self.mode = Mode::Select(SelectState::status_bulk(ids, statuses));
                } else if let Some(ticket) = self.tickets.get(self.selected) {
                    self.mode = Mode::Select(SelectState::status(
                        ticket.id.as_str().to_string(),
                        statuses,
                    ));
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Set assignees
            KeyCode::Char('U') => {
                if self.selection_mode && !self.selected_ids.is_empty() {
                    let ids = self.selected_ids_sorted();
                    self.mode = Mode::Input(InputState::set_assignees_bulk(ids));
                } else if let Some(ticket) = self.tickets.get(self.selected) {
                    self.mode =
                        Mode::Input(InputState::set_assignees(ticket.id.as_str().to_string()));
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Set milestone
            KeyCode::Char('M') => {
                if self.selection_mode && !self.selected_ids.is_empty() {
                    let ids = self.selected_ids_sorted();
                    self.open_milestone_menu(ids)?;
                } else if let Some(ticket) = self.tickets.get(self.selected) {
                    self.open_milestone_menu(vec![ticket.id.as_str().to_string()])?;
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Edit tags
            KeyCode::Char('t') => {
                if let Some(ticket) = self.tickets.get(self.selected) {
                    self.open_tag_edit(ticket.id.as_str().to_string())?;
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Search
            KeyCode::Char('/') => {
                self.mode = Mode::Search(SearchState::new(
                    self.search_query.clone(),
                    self.current_ticket_id(),
                ));
            }

            // Clear search or deselect all
            KeyCode::Char('x') => {
                if self.selection_mode && !self.selected_ids.is_empty() {
                    self.deselect_all();
                } else if !self.search_query.is_empty() {
                    let current_id = self.current_ticket_id();
                    self.search_query.clear();
                    self.refresh(current_id.as_deref())?;
                    self.status = "cleared search".to_string();
                }
            }

            // Multi-select: toggle selection
            KeyCode::Char(' ') => {
                self.toggle_selection();
                if !self.selection_mode {
                    self.selection_mode = true;
                }
                let count = self.selection_count();
                self.status = format!("{} ticket(s) selected", count);
            }

            // Multi-select: enter selection mode
            KeyCode::Char('V') => {
                self.select_all_visible();
            }

            // Edit
            KeyCode::Char('e') => {
                if let Some(ticket) = self.tickets.get(self.selected) {
                    let id = ticket.id.as_str().to_string();
                    self.open_ticket_edit(&id)?;
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }
            KeyCode::Char('o') => {
                self.request_edit_ticket();
            }
            KeyCode::Char('E') => {
                self.request_edit_notes();
            }

            // Help / Action menu
            KeyCode::Char('?') => {
                self.mode = Mode::Help;
            }
            KeyCode::Char('m') => {
                self.mode = Mode::Action(self.build_action_state());
            }

            // Dependency graph view
            KeyCode::Char('D') => {
                self.open_graph_view()?;
            }

            _ => {}
        }
        Ok(false)
    }

    /// Handle key in uninitialized state.
    fn handle_uninitialized_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('i') => {
                self.mode = Mode::Init(InitState::default());
            }
            KeyCode::Char('q') => return Ok(true),
            _ => {}
        }
        Ok(false)
    }

    /// Handle key in init mode (repo initialization with options).
    fn handle_init_mode(&mut self, key: KeyEvent) -> Result<bool> {
        let mut state = match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Init(state) => state,
            other => {
                self.mode = other;
                return Ok(false);
            }
        };

        match key.code {
            // Navigate between checkboxes
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => {
                state.focus = state.focus.next();
                self.mode = Mode::Init(state);
            }
            KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => {
                state.focus = state.focus.next(); // Only 2 items, next = prev
                self.mode = Mode::Init(state);
            }
            // Toggle the focused checkbox
            KeyCode::Char(' ') => {
                match state.focus {
                    InitFocus::Claude => state.setup_claude = !state.setup_claude,
                    InitFocus::Agents => state.setup_agents = !state.setup_agents,
                }
                self.mode = Mode::Init(state);
            }
            // Confirm initialization
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                self.perform_confirm_action(ConfirmAction::InitRepo {
                    setup_claude: state.setup_claude,
                    setup_agents: state.setup_agents,
                })?;
            }
            // Cancel
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.status = "cancelled".to_string();
            }
            KeyCode::Char('q') => return Ok(true),
            _ => {
                self.mode = Mode::Init(state);
            }
        }
        Ok(false)
    }

    /// Handle key in confirm mode.
    fn handle_confirm_mode(&mut self, key: KeyEvent) -> Result<bool> {
        let state = match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Confirm(state) => state,
            other => {
                self.mode = other;
                return Ok(false);
            }
        };

        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.perform_confirm_action(state.action)?;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.status = "cancelled".to_string();
            }
            KeyCode::Char('q') => return Ok(true),
            _ => {
                self.mode = Mode::Confirm(state);
            }
        }
        Ok(false)
    }

    /// Handle key in action mode.
    fn handle_action_mode(&mut self, key: KeyEvent) -> Result<bool> {
        let mut state = match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Action(state) => state,
            other => {
                self.mode = other;
                return Ok(false);
            }
        };

        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                return Ok(false);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if state.selected > 0 {
                    state.selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.selected + 1 < state.items.len() {
                    state.selected += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(item) = state.items.get(state.selected).cloned() {
                    if !item.enabled {
                        self.status = "action unavailable".to_string();
                    } else {
                        return self.perform_action(item.action);
                    }
                }
            }
            KeyCode::Char(ch) => {
                if let Some(item) = state.items.iter().find(|item| item.hotkey == ch).cloned() {
                    if !item.enabled {
                        self.status = "action unavailable".to_string();
                    } else {
                        return self.perform_action(item.action);
                    }
                }
            }
            _ => {}
        }

        self.mode = Mode::Action(state);
        Ok(false)
    }

    /// Handle key in search mode.
    fn handle_search_mode(&mut self, key: KeyEvent) -> Result<bool> {
        let mut state = match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Search(state) => state,
            other => {
                self.mode = other;
                return Ok(false);
            }
        };

        let mut changed = false;

        match key.code {
            KeyCode::Esc => {
                let restore_id = state.original_selected.clone();
                self.search_query = state.original.clone();
                self.refresh(restore_id.as_deref())?;
                self.mode = Mode::Normal;
                self.status = "search cancelled".to_string();
                return Ok(false);
            }
            KeyCode::Enter => {
                let current_id = self.current_ticket_id();
                self.search_query = state.query.trim().to_string();
                self.refresh(current_id.as_deref())?;
                if self.search_query.is_empty() {
                    self.status = "search cleared".to_string();
                } else {
                    self.status = format!("search: {}", self.search_query);
                }
                self.mode = Mode::Normal;
                return Ok(false);
            }
            KeyCode::Backspace => {
                if state.cursor > 0 {
                    remove_char(&mut state.query, state.cursor);
                    state.cursor -= 1;
                    changed = true;
                }
            }
            KeyCode::Left => {
                if state.cursor > 0 {
                    state.cursor -= 1;
                }
            }
            KeyCode::Right => {
                let len = state.query.chars().count();
                if state.cursor < len {
                    state.cursor += 1;
                }
            }
            KeyCode::Home => {
                state.cursor = 0;
            }
            KeyCode::End => {
                state.cursor = state.query.chars().count();
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(true);
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Next match
                if !state.match_indices.is_empty() {
                    state.current_match = (state.current_match + 1) % state.match_indices.len();
                    self.selected = state.match_indices[state.current_match];
                }
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Previous match
                if !state.match_indices.is_empty() {
                    if state.current_match == 0 {
                        state.current_match = state.match_indices.len() - 1;
                    } else {
                        state.current_match -= 1;
                    }
                    self.selected = state.match_indices[state.current_match];
                }
            }
            KeyCode::Char(ch) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    insert_char(&mut state.query, state.cursor, ch);
                    state.cursor += 1;
                    changed = true;
                }
            }
            _ => {}
        }

        if changed {
            let current_id = self.current_ticket_id();
            self.search_query = state.query.trim().to_string();
            self.refresh(current_id.as_deref())?;

            // Update match indices
            state.match_indices = self
                .tickets
                .iter()
                .enumerate()
                .filter(|(_, t)| ticket_matches_query(t, &state.query))
                .map(|(i, _)| i)
                .collect();
            state.current_match = 0;
        }
        self.mode = Mode::Search(state);
        Ok(false)
    }

    /// Handle key in select mode.
    fn handle_select_mode(&mut self, key: KeyEvent) -> Result<bool> {
        let mut state = match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Select(state) => state,
            other => {
                self.mode = other;
                return Ok(false);
            }
        };

        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                return Ok(false);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if state.selected > 0 {
                    state.selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.selected + 1 < state.options.len() {
                    state.selected += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(opt) = state.options.get(state.selected) {
                    return self.apply_selection(&state.kind, &opt.value);
                }
            }
            KeyCode::Char(ch) => {
                if let Some(opt) = state.options.iter().find(|o| o.hotkey == Some(ch)) {
                    return self.apply_selection(&state.kind, &opt.value);
                }
            }
            _ => {}
        }

        self.mode = Mode::Select(state);
        Ok(false)
    }

    /// Handle key in help mode.
    fn handle_help_mode(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        Ok(false)
    }

    /// Handle key in milestone menu mode.
    fn handle_milestone_mode(&mut self, key: KeyEvent) -> Result<bool> {
        let mut state = match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::MilestoneMenu(state) => state,
            other => {
                self.mode = other;
                return Ok(false);
            }
        };

        let total = 1 + state.milestones.len(); // 1 for "clear" option
        if state.ticket_ids.is_empty() {
            self.status = "no ticket selected".to_string();
            self.mode = Mode::Normal;
            return Ok(false);
        }

        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                return Ok(false);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if state.selected > 0 {
                    state.selected -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.selected + 1 < total {
                    state.selected += 1;
                }
            }
            KeyCode::Char('x') => {
                // Quick clear
                if state.ticket_ids.len() > 1 {
                    return self.confirm_bulk_milestone(state.ticket_ids.clone(), None);
                }
                return self.set_ticket_milestone(&state.ticket_ids[0], None);
            }
            KeyCode::Enter => {
                if state.selected == 0 {
                    // Clear milestone
                    if state.ticket_ids.len() > 1 {
                        return self.confirm_bulk_milestone(state.ticket_ids.clone(), None);
                    }
                    return self.set_ticket_milestone(&state.ticket_ids[0], None);
                } else {
                    // Set milestone
                    let milestone = &state.milestones[state.selected - 1];
                    if state.ticket_ids.len() > 1 {
                        return self.confirm_bulk_milestone(
                            state.ticket_ids.clone(),
                            Some(milestone.id.as_str().to_string()),
                        );
                    }
                    return self.set_ticket_milestone(
                        &state.ticket_ids[0],
                        Some(milestone.id.as_str()),
                    );
                }
            }
            _ => {}
        }

        self.mode = Mode::MilestoneMenu(state);
        Ok(false)
    }

    /// Handle key in input mode.
    pub fn handle_input_mode(&mut self, key: KeyEvent) -> Result<bool> {
        let mut state = match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Input(state) => state,
            other => {
                self.mode = other;
                return Ok(false);
            }
        };
        let mut submit = false;
        let mut exit = false;
        let mut restore = true;

        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.status = "cancelled".to_string();
                restore = false;
            }
            KeyCode::Enter => {
                if state.current + 1 == state.fields.len() {
                    submit = true;
                } else {
                    state.current += 1;
                    state.cursor = state.fields[state.current].value.chars().count();
                }
            }
            KeyCode::Tab | KeyCode::Down => {
                if state.current + 1 < state.fields.len() {
                    state.current += 1;
                    state.cursor = state.fields[state.current].value.chars().count();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if state.current > 0 {
                    state.current -= 1;
                    state.cursor = state.fields[state.current].value.chars().count();
                }
            }
            KeyCode::Backspace => {
                let field = &mut state.fields[state.current];
                if state.cursor > 0 {
                    remove_char(&mut field.value, state.cursor);
                    state.cursor -= 1;
                }
            }
            KeyCode::Left => {
                if state.cursor > 0 {
                    state.cursor -= 1;
                }
            }
            KeyCode::Right => {
                let field = &state.fields[state.current];
                let len = field.value.chars().count();
                if state.cursor < len {
                    state.cursor += 1;
                }
            }
            KeyCode::Home => {
                state.cursor = 0;
            }
            KeyCode::End => {
                state.cursor = state.fields[state.current].value.chars().count();
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                exit = true;
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                submit = true;
            }
            KeyCode::Char(ch) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    let field = &mut state.fields[state.current];
                    insert_char(&mut field.value, state.cursor, ch);
                    state.cursor += 1;
                }
            }
            _ => {}
        }
        if exit {
            return Ok(true);
        }
        if submit {
            if self.submit_input(&state)? {
                if matches!(self.mode, Mode::Normal) {
                    self.mode = Mode::Normal;
                }
                return Ok(false);
            }
            if matches!(self.mode, Mode::Normal) {
                self.mode = Mode::Input(state);
            }
            return Ok(false);
        }
        if restore {
            self.mode = Mode::Input(state);
        }
        Ok(false)
    }

    /// Submit input form.
    fn submit_input(&mut self, state: &InputState) -> Result<bool> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(false);
        };

        match &state.kind {
            InputKind::NewTicket => {
                let title = state.fields[0].value.trim();
                if title.is_empty() {
                    self.status = "title is required".to_string();
                    return Ok(false);
                }
                let summary = state.fields[1].value.trim();
                let description = state.fields[2].value.trim();
                let tags = parse_csv_list(&state.fields[3].value);
                let actor = resolve_actor();
                let ticket = repo.create_ticket(
                    tik_core::NewTicket {
                        title: title.to_string(),
                        summary: if summary.is_empty() {
                            None
                        } else {
                            Some(summary.to_string())
                        },
                        description: if description.is_empty() {
                            None
                        } else {
                            Some(description.to_string())
                        },
                        tags,
                    },
                    &actor,
                )?;
                self.refresh(Some(ticket.id.as_str()))?;
                self.status = format!("created {}", ticket.id.as_str());
                Ok(true)
            }
            InputKind::AddNote(id) => {
                let text = state.fields[0].value.trim();
                if text.is_empty() {
                    self.status = "note text is required".to_string();
                    return Ok(false);
                }
                let actor = resolve_actor();
                let ticket_id = TicketId::parse(id)?;
                repo.append_note(&ticket_id, &actor, text)?;
                self.refresh(Some(id))?;
                self.load_ticket_details()?;
                self.status = format!("added note to {id}");
                Ok(true)
            }
            InputKind::EditTicket(id) => {
                let title = state.fields[0].value.trim();
                if title.is_empty() {
                    self.status = "title is required".to_string();
                    return Ok(false);
                }
                let summary_raw = state.fields[1].value.trim();
                let summary = if summary_raw.is_empty() {
                    title.to_string()
                } else {
                    summary_raw.to_string()
                };
                let description = state.fields[2].value.trim().to_string();
                let tags = parse_csv_list(&state.fields[3].value);
                let assignees = parse_csv_list(&state.fields[4].value);
                let Some(priority) = parse_priority(&state.fields[5].value) else {
                    self.status = "invalid priority (low|medium|high|critical)".to_string();
                    return Ok(false);
                };
                let Some(severity) = parse_severity(&state.fields[6].value) else {
                    self.status = "invalid severity (low|normal|high|critical)".to_string();
                    return Ok(false);
                };
                let Some(status) = parse_status(&state.fields[7].value) else {
                    self.status = "invalid status (open|in_progress|blocked|closed|archived)"
                        .to_string();
                    return Ok(false);
                };
                let milestone_raw = state.fields[8].value.trim();
                let milestone_id = if milestone_raw.is_empty() {
                    None
                } else {
                    match MilestoneId::parse(milestone_raw) {
                        Ok(mid) => Some(mid),
                        Err(_) => {
                            self.status = "invalid milestone id".to_string();
                            return Ok(false);
                        }
                    }
                };
                if let Some(mid) = &milestone_id {
                    if repo.load_milestone(mid).is_err() {
                        self.status = format!("milestone not found: {}", mid.as_str());
                        return Ok(false);
                    }
                }

                let actor = resolve_actor();
                let ticket_id = TicketId::parse(id)?;
                let existing = repo.load_ticket(&ticket_id)?;
                let mut changed = false;

                if existing.title != title
                    || existing.summary != summary
                    || existing.description != description
                    || existing.tags != tags
                    || existing.assignees != assignees
                    || existing.priority != priority
                    || existing.severity != severity
                {
                    apply_ticket_edit(repo, &ticket_id, &actor, "edited in TUI", |ticket| {
                        ticket.title = title.to_string();
                        ticket.summary = summary.clone();
                        ticket.description = description.clone();
                        ticket.tags = tags.clone();
                        ticket.assignees = assignees.clone();
                        ticket.priority = priority.clone();
                        ticket.severity = severity.clone();
                    })?;
                    changed = true;
                }

                if existing.status != status {
                    repo.update_status(&ticket_id, status.clone(), &actor, Some("edited in TUI"))?;
                    changed = true;
                }

                let existing_milestone = existing
                    .milestone_id
                    .as_ref()
                    .map(|mid| mid.as_str().to_string());
                let new_milestone = milestone_id.as_ref().map(|mid| mid.as_str().to_string());
                if existing_milestone != new_milestone {
                    repo.set_ticket_milestone(
                        &ticket_id,
                        milestone_id.as_ref(),
                        &actor,
                        Some("edited in TUI"),
                    )?;
                    changed = true;
                }

                if !changed {
                    self.status = "no changes".to_string();
                    return Ok(true);
                }

                self.refresh(Some(id))?;
                self.ticket_details.ticket_id = None;
                self.load_ticket_details()?;
                self.status = format!("updated {id}");
                Ok(true)
            }
            InputKind::CloseReason(id) => {
                let reason = state.fields[0].value.trim();
                let actor = resolve_actor();
                let ticket_id = TicketId::parse(id)?;
                repo.update_status(
                    &ticket_id,
                    TicketStatus::Closed,
                    &actor,
                    if reason.is_empty() {
                        None
                    } else {
                        Some(reason)
                    },
                )?;
                self.refresh(Some(id))?;
                self.status = format!("closed {id}");
                Ok(true)
            }
            InputKind::ReopenReason(id) => {
                let reason = state.fields[0].value.trim();
                let actor = resolve_actor();
                let ticket_id = TicketId::parse(id)?;
                repo.update_status(
                    &ticket_id,
                    TicketStatus::Open,
                    &actor,
                    if reason.is_empty() {
                        None
                    } else {
                        Some(reason)
                    },
                )?;
                self.refresh(Some(id))?;
                self.status = format!("reopened {id}");
                Ok(true)
            }
            InputKind::SetAssignees(id) => {
                let assignees = parse_csv_list(&state.fields[0].value);
                let actor = resolve_actor();
                let ticket_id = TicketId::parse(id)?;
                repo.set_assignees(&ticket_id, assignees, &actor, Some("set assignees"))?;
                self.refresh(Some(id))?;
                self.status = format!("assignees updated for {id}");
                Ok(true)
            }
            InputKind::SetAssigneesBulk(ids) => {
                let assignees = parse_csv_list(&state.fields[0].value);
                let ids = ids.clone();
                let prompt = if assignees.is_empty() {
                    format!("Clear assignees for {} selected ticket(s)?", ids.len())
                } else {
                    format!(
                        "Set assignees to {} for {} selected ticket(s)?",
                        assignees.join(", "),
                        ids.len()
                    )
                };
                self.mode = Mode::Confirm(ConfirmState {
                    prompt,
                    action: ConfirmAction::BulkSetAssignees { ids, assignees },
                });
                Ok(true)
            }
        }
    }

    /// Apply a selection from select menu.
    fn apply_selection(&mut self, kind: &SelectKind, value: &str) -> Result<bool> {
        match kind {
            SelectKind::Priority(id) => {
                let Some(priority) = parse_priority(value) else {
                    self.status = "invalid priority".to_string();
                    self.mode = Mode::Normal;
                    return Ok(false);
                };
                let Some(repo) = &self.repo else {
                    self.status = "repo not initialized".to_string();
                    self.mode = Mode::Normal;
                    return Ok(false);
                };
                let actor = resolve_actor();
                let ticket_id = TicketId::parse(id)?;
                apply_ticket_edit(repo, &ticket_id, &actor, "set priority", |ticket| {
                    ticket.priority = priority.clone();
                })?;
                self.refresh(Some(id))?;
                self.status = format!("priority set to {}", priority.as_str());
                self.mode = Mode::Normal;
            }
            SelectKind::Severity(id) => {
                let Some(severity) = parse_severity(value) else {
                    self.status = "invalid severity".to_string();
                    self.mode = Mode::Normal;
                    return Ok(false);
                };
                let Some(repo) = &self.repo else {
                    self.status = "repo not initialized".to_string();
                    self.mode = Mode::Normal;
                    return Ok(false);
                };
                let actor = resolve_actor();
                let ticket_id = TicketId::parse(id)?;
                apply_ticket_edit(repo, &ticket_id, &actor, "set severity", |ticket| {
                    ticket.severity = severity.clone();
                })?;
                self.refresh(Some(id))?;
                self.status = format!("severity set to {}", severity.as_str());
                self.mode = Mode::Normal;
            }
            SelectKind::Status(id) => {
                let Some(status) = parse_status(value) else {
                    self.status = "invalid status".to_string();
                    self.mode = Mode::Normal;
                    return Ok(false);
                };
                let Some(repo) = &self.repo else {
                    self.status = "repo not initialized".to_string();
                    self.mode = Mode::Normal;
                    return Ok(false);
                };
                let actor = resolve_actor();
                let ticket_id = TicketId::parse(id)?;
                repo.update_status(&ticket_id, status.clone(), &actor, Some("set status"))?;
                self.refresh(Some(id))?;
                self.status = format!("status set to {}", status.as_str());
                self.mode = Mode::Normal;
            }
            SelectKind::BulkPriority(ids) => {
                let Some(priority) = parse_priority(value) else {
                    self.status = "invalid priority".to_string();
                    self.mode = Mode::Normal;
                    return Ok(false);
                };
                let ids = ids.clone();
                self.mode = Mode::Confirm(ConfirmState {
                    prompt: format!(
                        "Set priority to {} for {} selected ticket(s)?",
                        priority.as_str(),
                        ids.len()
                    ),
                    action: ConfirmAction::BulkSetPriority { ids, priority },
                });
            }
            SelectKind::BulkSeverity(ids) => {
                let Some(severity) = parse_severity(value) else {
                    self.status = "invalid severity".to_string();
                    self.mode = Mode::Normal;
                    return Ok(false);
                };
                let ids = ids.clone();
                self.mode = Mode::Confirm(ConfirmState {
                    prompt: format!(
                        "Set severity to {} for {} selected ticket(s)?",
                        severity.as_str(),
                        ids.len()
                    ),
                    action: ConfirmAction::BulkSetSeverity { ids, severity },
                });
            }
            SelectKind::BulkStatus(ids) => {
                let Some(status) = parse_status(value) else {
                    self.status = "invalid status".to_string();
                    self.mode = Mode::Normal;
                    return Ok(false);
                };
                let ids = ids.clone();
                self.mode = Mode::Confirm(ConfirmState {
                    prompt: format!(
                        "Set status to {} for {} selected ticket(s)?",
                        status.as_str(),
                        ids.len()
                    ),
                    action: ConfirmAction::BulkSetStatus { ids, status },
                });
            }
            SelectKind::Filter => {
                let filter = match value {
                    "all" => TicketFilter::All,
                    "open" => TicketFilter::Open,
                    "in_progress" => TicketFilter::InProgress,
                    "blocked" => TicketFilter::Blocked,
                    "closed" => TicketFilter::Closed,
                    "archived" => TicketFilter::Archived,
                    _ => TicketFilter::All,
                };
                let current_id = self.current_ticket_id();
                self.filter = filter;
                self.refresh(current_id.as_deref())?;
                self.status = format!("filter: {}", self.filter.label());
                self.mode = Mode::Normal;
            }
        }
        Ok(false)
    }

    /// Set milestone for a ticket.
    fn set_ticket_milestone(
        &mut self,
        ticket_id: &str,
        milestone_id: Option<&str>,
    ) -> Result<bool> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            self.mode = Mode::Normal;
            return Ok(false);
        };

        let actor = resolve_actor();
        let tid = TicketId::parse(ticket_id)?;
        let mid = milestone_id.map(MilestoneId::parse).transpose()?;

        repo.set_ticket_milestone(&tid, mid.as_ref(), &actor, Some("set milestone"))?;
        self.refresh(Some(ticket_id))?;

        if let Some(mid) = milestone_id {
            self.status = format!("milestone set to {}", mid);
        } else {
            self.status = "milestone cleared".to_string();
        }
        self.mode = Mode::Normal;
        Ok(false)
    }

    /// Open ticket edit form.
    fn open_ticket_edit(&mut self, ticket_id: &str) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };

        let tid = TicketId::parse(ticket_id)?;
        let ticket = repo.load_ticket(&tid)?;
        self.mode = Mode::Input(InputState::edit_ticket(&ticket));
        Ok(())
    }

    /// Open milestone menu.
    fn open_milestone_menu(&mut self, ticket_ids: Vec<String>) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };

        let milestones = repo.list_milestones(None)?;
        self.mode = Mode::MilestoneMenu(MilestoneMenuState {
            ticket_ids,
            milestones,
            selected: 0,
        });
        Ok(())
    }

    /// Open tag edit mode.
    fn open_tag_edit(&mut self, ticket_id: String) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };

        let ticket = repo.load_ticket(&TicketId::parse(&ticket_id)?)?;
        let current_tags = ticket.tags.clone();

        // Gather all known tags for autocomplete
        let all_tickets = repo.list_tickets(None)?;
        let mut all_tags: Vec<String> = all_tickets
            .iter()
            .flat_map(|t| t.tags.iter().cloned())
            .collect();
        all_tags.sort();
        all_tags.dedup();

        self.mode = Mode::TagEdit(TagEditState::new(ticket_id, current_tags, all_tags));
        Ok(())
    }

    /// Handle key in tag edit mode.
    fn handle_tag_edit_mode(&mut self, key: KeyEvent) -> Result<bool> {
        let mut state = match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::TagEdit(state) => state,
            other => {
                self.mode = other;
                return Ok(false);
            }
        };

        match key.code {
            KeyCode::Esc => {
                // Cancel - restore original tags
                self.mode = Mode::Normal;
                self.status = "tag edit cancelled".to_string();
                return Ok(false);
            }
            KeyCode::Enter => {
                if state.suggestion_idx.is_some() && !state.suggestions.is_empty() {
                    // Add selected suggestion
                    state.add_selected_suggestion();
                } else if !state.input.is_empty() {
                    // Add typed tag
                    state.add_current_tag();
                } else {
                    // Submit changes
                    return self.submit_tag_edit(&state);
                }
            }
            KeyCode::Tab => {
                // Cycle through suggestions or add selected
                if !state.suggestions.is_empty() {
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        state.prev_suggestion();
                    } else {
                        state.next_suggestion();
                    }
                }
            }
            KeyCode::Backspace => {
                if state.cursor > 0 {
                    remove_char(&mut state.input, state.cursor);
                    state.cursor -= 1;
                    state.update_suggestions();
                } else {
                    // Remove last tag when backspace on empty input
                    if state.remove_last_tag() {
                        // Tag removed
                    }
                }
            }
            KeyCode::Left => {
                if state.cursor > 0 {
                    state.cursor -= 1;
                }
            }
            KeyCode::Right => {
                let len = state.input.chars().count();
                if state.cursor < len {
                    state.cursor += 1;
                }
            }
            KeyCode::Home => {
                state.cursor = 0;
            }
            KeyCode::End => {
                state.cursor = state.input.chars().count();
            }
            KeyCode::Up => {
                state.prev_suggestion();
            }
            KeyCode::Down => {
                state.next_suggestion();
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(true);
            }
            KeyCode::Char(ch) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    insert_char(&mut state.input, state.cursor, ch);
                    state.cursor += 1;
                    state.update_suggestions();
                }
            }
            _ => {}
        }

        self.mode = Mode::TagEdit(state);
        Ok(false)
    }

    /// Handle key in relation edit mode.
    fn handle_relation_edit_mode(&mut self, key: KeyEvent) -> Result<bool> {
        let mut state = match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::RelationEdit(state) => state,
            other => {
                self.mode = other;
                return Ok(false);
            }
        };

        match state.step {
            RelationStep::SelectType => {
                match key.code {
                    KeyCode::Esc => {
                        self.mode = Mode::Normal;
                        self.status = "relation edit cancelled".to_string();
                        return Ok(false);
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.prev_type();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        state.next_type();
                    }
                    KeyCode::Enter => {
                        state.select_type();
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(true);
                    }
                    _ => {}
                }
            }
            RelationStep::SearchTarget => {
                match key.code {
                    KeyCode::Esc => {
                        // Go back to type selection
                        state.step = RelationStep::SelectType;
                        state.relation_type = None;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.prev_ticket();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        state.next_ticket();
                    }
                    KeyCode::Enter => {
                        // Submit the relation
                        if let (Some(rel_type), Some(target)) =
                            (state.relation_type.clone(), state.selected_target())
                        {
                            return self.submit_relation(&state.ticket_id, rel_type, &target.id);
                        }
                    }
                    KeyCode::Backspace => {
                        if state.search_cursor > 0 {
                            remove_char(&mut state.search_query, state.search_cursor);
                            state.search_cursor -= 1;
                            state.update_filter();
                        }
                    }
                    KeyCode::Left => {
                        if state.search_cursor > 0 {
                            state.search_cursor -= 1;
                        }
                    }
                    KeyCode::Right => {
                        let len = state.search_query.chars().count();
                        if state.search_cursor < len {
                            state.search_cursor += 1;
                        }
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(true);
                    }
                    KeyCode::Char(ch) => {
                        if !key.modifiers.contains(KeyModifiers::CONTROL) {
                            insert_char(&mut state.search_query, state.search_cursor, ch);
                            state.search_cursor += 1;
                            state.update_filter();
                        }
                    }
                    _ => {}
                }
            }
        }

        self.mode = Mode::RelationEdit(state);
        Ok(false)
    }

    /// Submit a new relation.
    fn submit_relation(
        &mut self,
        ticket_id: &str,
        relation_type: tik_core::RelationType,
        target_id: &str,
    ) -> Result<bool> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            self.mode = Mode::Normal;
            return Ok(false);
        };

        let actor = resolve_actor();
        let tid = TicketId::parse(ticket_id)?;
        let target_tid = TicketId::parse(target_id)?;

        repo.add_relation(&tid, relation_type.clone(), &target_tid, &actor, None)?;

        self.refresh(Some(ticket_id))?;
        self.ticket_details.ticket_id = None;
        self.load_ticket_details()?;

        self.status = format!(
            "added {} relation to {}",
            relation_type.as_str(),
            target_id
        );
        self.mode = Mode::Normal;
        Ok(false)
    }

    /// Open relation edit mode.
    fn open_relation_edit(&mut self, ticket_id: String) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };

        // Get all tickets for selection
        let all_tickets = repo.list_tickets(None)?;
        let summaries: Vec<TicketSummary> = all_tickets
            .into_iter()
            .map(|t| TicketSummary {
                id: t.id.as_str().to_string(),
                title: t.title,
                status: t.status,
            })
            .collect();

        self.mode = Mode::RelationEdit(RelationEditState::new(ticket_id, summaries));
        Ok(())
    }

    /// Remove the currently selected relation.
    fn remove_selected_relation(&mut self) -> Result<()> {
        let Some(ticket) = self.tickets.get(self.selected) else {
            self.status = "no ticket selected".to_string();
            return Ok(());
        };

        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };

        let relation_idx = self.ticket_details.relation_selected;
        let Some(relation) = ticket.relations.get(relation_idx) else {
            self.status = "no relation selected".to_string();
            return Ok(());
        };

        let actor = resolve_actor();
        let tid = TicketId::parse(ticket.id.as_str())?;

        repo.remove_relation(&tid, relation.kind.clone(), &relation.id, &actor, None)?;

        let ticket_id = ticket.id.as_str().to_string();
        self.refresh(Some(&ticket_id))?;
        self.ticket_details.ticket_id = None;
        self.load_ticket_details()?;

        // Adjust selection if needed
        if let Some(ticket) = self.tickets.get(self.selected) {
            if self.ticket_details.relation_selected >= ticket.relations.len() {
                self.ticket_details.relation_selected =
                    ticket.relations.len().saturating_sub(1);
            }
        }

        self.status = "relation removed".to_string();
        Ok(())
    }

    /// Jump to the target ticket of the selected relation.
    fn jump_to_selected_relation(&mut self) -> Result<()> {
        let Some(ticket) = self.tickets.get(self.selected) else {
            return Ok(());
        };

        let relation_idx = self.ticket_details.relation_selected;
        let Some(relation) = ticket.relations.get(relation_idx) else {
            self.status = "no relation to jump to".to_string();
            return Ok(());
        };

        let target_id = relation.id.as_str().to_string();

        // Refresh with the target ID
        self.refresh(Some(&target_id))?;
        self.ticket_details.ticket_id = None;
        self.load_ticket_details()?;
        self.focused_panel = Panel::TicketList;

        self.status = format!("jumped to {}", target_id);
        Ok(())
    }

    /// Handle key in graph view mode.
    fn handle_graph_view_mode(&mut self, key: KeyEvent) -> Result<bool> {
        let mut state = match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::GraphView(state) => state,
            other => {
                self.mode = other;
                return Ok(false);
            }
        };

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
                self.status = "closed graph view".to_string();
                return Ok(false);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.prev_node();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.next_node();
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                for _ in 0..10 {
                    state.scroll_down();
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                for _ in 0..10 {
                    state.scroll_up();
                }
            }
            KeyCode::Enter => {
                // Jump to the focused node
                if let Some(node_id) = state.focused_node_id() {
                    let id = node_id.to_string();
                    self.mode = Mode::Normal;
                    self.refresh(Some(&id))?;
                    self.ticket_details.ticket_id = None;
                    self.load_ticket_details()?;
                    self.status = format!("jumped to {}", id);
                    return Ok(false);
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(true);
            }
            _ => {}
        }

        self.mode = Mode::GraphView(state);
        Ok(false)
    }

    /// Submit tag edit changes.
    fn submit_tag_edit(&mut self, state: &TagEditState) -> Result<bool> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            self.mode = Mode::Normal;
            return Ok(false);
        };

        let ticket_id = TicketId::parse(&state.ticket_id)?;
        let actor = resolve_actor();

        // Set the new tags
        repo.set_tags(&ticket_id, state.tags.clone(), &actor, Some("edited tags"))?;

        self.refresh(Some(&state.ticket_id))?;
        self.ticket_details.ticket_id = None;
        self.load_ticket_details()?;

        let added = state
            .tags
            .iter()
            .filter(|t| !state.original_tags.contains(t))
            .count();
        let removed = state
            .original_tags
            .iter()
            .filter(|t| !state.tags.contains(t))
            .count();

        self.status = format!("tags updated (+{} -{}) for {}", added, removed, state.ticket_id);
        self.mode = Mode::Normal;
        Ok(false)
    }

    /// Bulk close all selected tickets.
    fn bulk_close_ids(&mut self, ids: Vec<String>) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };

        let actor = resolve_actor();
        let count = ids.len();
        let mut closed = 0;

        for id in ids {
            let ticket_id = TicketId::parse(&id)?;
            if repo
                .update_status(&ticket_id, TicketStatus::Closed, &actor, Some("bulk close"))
                .is_ok()
            {
                closed += 1;
            }
        }

        self.refresh(None)?;
        self.exit_selection_mode();
        self.status = format!("closed {}/{} tickets", closed, count);
        Ok(())
    }

    /// Bulk reopen all selected tickets.
    fn bulk_reopen_ids(&mut self, ids: Vec<String>) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };

        let actor = resolve_actor();
        let count = ids.len();
        let mut reopened = 0;

        for id in ids {
            let ticket_id = TicketId::parse(&id)?;
            if repo
                .update_status(&ticket_id, TicketStatus::Open, &actor, Some("bulk reopen"))
                .is_ok()
            {
                reopened += 1;
            }
        }

        self.refresh(None)?;
        self.exit_selection_mode();
        self.status = format!("reopened {}/{} tickets", reopened, count);
        Ok(())
    }

    /// Confirm bulk milestone change.
    fn confirm_bulk_milestone(
        &mut self,
        ids: Vec<String>,
        milestone_id: Option<String>,
    ) -> Result<bool> {
        let prompt = if let Some(mid) = &milestone_id {
            format!(
                "Set milestone {} for {} selected ticket(s)?",
                mid,
                ids.len()
            )
        } else {
            format!("Clear milestone for {} selected ticket(s)?", ids.len())
        };
        self.mode = Mode::Confirm(ConfirmState {
            prompt,
            action: ConfirmAction::BulkSetMilestone { ids, milestone_id },
        });
        Ok(false)
    }

    /// Apply a bulk priority update.
    fn bulk_set_priority(&mut self, ids: Vec<String>, priority: Priority) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };
        let actor = resolve_actor();
        let count = ids.len();
        let mut updated = 0;

        for id in ids {
            let ticket_id = TicketId::parse(&id)?;
            if apply_ticket_edit(repo, &ticket_id, &actor, "bulk set priority", |ticket| {
                ticket.priority = priority.clone();
            })
            .is_ok()
            {
                updated += 1;
            }
        }

        self.refresh(None)?;
        self.exit_selection_mode();
        self.status = format!(
            "priority set to {} for {}/{} tickets",
            priority.as_str(),
            updated,
            count
        );
        Ok(())
    }

    /// Apply a bulk severity update.
    fn bulk_set_severity(&mut self, ids: Vec<String>, severity: Severity) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };
        let actor = resolve_actor();
        let count = ids.len();
        let mut updated = 0;

        for id in ids {
            let ticket_id = TicketId::parse(&id)?;
            if apply_ticket_edit(repo, &ticket_id, &actor, "bulk set severity", |ticket| {
                ticket.severity = severity.clone();
            })
            .is_ok()
            {
                updated += 1;
            }
        }

        self.refresh(None)?;
        self.exit_selection_mode();
        self.status = format!(
            "severity set to {} for {}/{} tickets",
            severity.as_str(),
            updated,
            count
        );
        Ok(())
    }

    /// Apply a bulk status update.
    fn bulk_set_status(&mut self, ids: Vec<String>, status: TicketStatus) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };
        let actor = resolve_actor();
        let count = ids.len();
        let mut updated = 0;

        for id in ids {
            let ticket_id = TicketId::parse(&id)?;
            if repo
                .update_status(&ticket_id, status.clone(), &actor, Some("bulk set status"))
                .is_ok()
            {
                updated += 1;
            }
        }

        self.refresh(None)?;
        self.exit_selection_mode();
        self.status = format!(
            "status set to {} for {}/{} tickets",
            status.as_str(),
            updated,
            count
        );
        Ok(())
    }

    /// Apply a bulk assignees update.
    fn bulk_set_assignees(&mut self, ids: Vec<String>, assignees: Vec<String>) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };
        let actor = resolve_actor();
        let count = ids.len();
        let mut updated = 0;

        for id in ids {
            let ticket_id = TicketId::parse(&id)?;
            if repo
                .set_assignees(
                    &ticket_id,
                    assignees.clone(),
                    &actor,
                    Some("bulk set assignees"),
                )
                .is_ok()
            {
                updated += 1;
            }
        }

        self.refresh(None)?;
        self.exit_selection_mode();
        self.status = format!("assignees updated for {}/{} tickets", updated, count);
        Ok(())
    }

    /// Apply a bulk milestone update.
    fn bulk_set_milestone(
        &mut self,
        ids: Vec<String>,
        milestone_id: Option<String>,
    ) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };
        let actor = resolve_actor();
        let mid = milestone_id
            .as_ref()
            .map(|value| MilestoneId::parse(value))
            .transpose()?;
        let count = ids.len();
        let mut updated = 0;

        for id in ids {
            let ticket_id = TicketId::parse(&id)?;
            if repo
                .set_ticket_milestone(&ticket_id, mid.as_ref(), &actor, Some("bulk set milestone"))
                .is_ok()
            {
                updated += 1;
            }
        }

        self.refresh(None)?;
        self.exit_selection_mode();
        if let Some(mid) = milestone_id {
            self.status = format!("milestone set to {} for {}/{} tickets", mid, updated, count);
        } else {
            self.status = format!("milestone cleared for {}/{} tickets", updated, count);
        }
        Ok(())
    }

    fn selected_ids_sorted(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.selected_ids.iter().cloned().collect();
        ids.sort();
        ids
    }

    /// Perform confirmed action.
    fn perform_confirm_action(&mut self, action: ConfirmAction) -> Result<()> {
        match action {
            ConfirmAction::InitRepo {
                setup_claude,
                setup_agents,
            } => {
                let repo = tik_core::Repo::init(&self.root, env!("CARGO_PKG_VERSION"))?;
                self.repo = Some(repo);
                self.refresh(None)?;

                if setup_claude || setup_agents {
                    let result =
                        crate::claude_setup::setup_ai_integration(&self.root, setup_claude, setup_agents)?;
                    self.status = format!("initialized repo. {}", result.summary());
                } else {
                    self.status = "initialized repo".to_string();
                }
            }
            ConfirmAction::Close(id) => {
                self.mode = Mode::Input(InputState::close_reason(id));
            }
            ConfirmAction::Reopen(id) => {
                self.mode = Mode::Input(InputState::reopen_reason(id));
            }
            ConfirmAction::BulkClose(ids) => {
                self.bulk_close_ids(ids)?;
            }
            ConfirmAction::BulkReopen(ids) => {
                self.bulk_reopen_ids(ids)?;
            }
            ConfirmAction::BulkSetPriority { ids, priority } => {
                self.bulk_set_priority(ids, priority)?;
            }
            ConfirmAction::BulkSetSeverity { ids, severity } => {
                self.bulk_set_severity(ids, severity)?;
            }
            ConfirmAction::BulkSetStatus { ids, status } => {
                self.bulk_set_status(ids, status)?;
            }
            ConfirmAction::BulkSetAssignees { ids, assignees } => {
                self.bulk_set_assignees(ids, assignees)?;
            }
            ConfirmAction::BulkSetMilestone {
                ids,
                milestone_id,
            } => {
                self.bulk_set_milestone(ids, milestone_id)?;
            }
        }
        Ok(())
    }

    /// Perform action from action menu.
    fn perform_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::NewTicket => {
                self.mode = Mode::Input(InputState::new_ticket());
            }
            Action::AddNote(id) => {
                if id.is_empty() {
                    self.status = "no ticket selected".to_string();
                } else {
                    self.mode = Mode::Input(InputState::add_note(id));
                }
            }
            Action::EditTicket(id) => {
                if id.is_empty() {
                    self.status = "no ticket selected".to_string();
                } else {
                    self.open_ticket_edit(&id)?;
                }
            }
            Action::EditTicketEditor(id) => {
                self.mode = Mode::Normal;
                self.request_edit_ticket_id(&id);
            }
            Action::EditNotes(id) => {
                self.mode = Mode::Normal;
                self.request_edit_notes_id(&id);
            }
            Action::Close(id) => {
                if id.is_empty() {
                    self.status = "no ticket selected".to_string();
                } else {
                    self.request_close(id);
                }
            }
            Action::Reopen(id) => {
                if id.is_empty() {
                    self.status = "no ticket selected".to_string();
                } else {
                    self.request_reopen(id);
                }
            }
            Action::SetPriority(id) => {
                if id.is_empty() {
                    self.status = "no ticket selected".to_string();
                } else {
                    let Some(repo) = &self.repo else {
                        self.status = "repo not initialized".to_string();
                        return Ok(false);
                    };
                    let priorities = repo.config_show()?.ticket_priorities;
                    self.mode = Mode::Select(SelectState::priority(id, priorities));
                }
            }
            Action::SetSeverity(id) => {
                if id.is_empty() {
                    self.status = "no ticket selected".to_string();
                } else {
                    let Some(repo) = &self.repo else {
                        self.status = "repo not initialized".to_string();
                        return Ok(false);
                    };
                    let severities = repo.config_show()?.ticket_severities;
                    self.mode = Mode::Select(SelectState::severity(id, severities));
                }
            }
            Action::SetAssignees(id) => {
                if id.is_empty() {
                    self.status = "no ticket selected".to_string();
                } else {
                    self.mode = Mode::Input(InputState::set_assignees(id));
                }
            }
            Action::ToggleFilter => {
                let current_id = self.current_ticket_id();
                self.filter = self.filter.next();
                self.refresh(current_id.as_deref())?;
                self.status = format!("filter: {}", self.filter.label());
                self.mode = Mode::Normal;
            }
            Action::Refresh => {
                self.refresh(None)?;
                self.status = "refreshed".to_string();
                self.mode = Mode::Normal;
            }
            Action::Search => {
                self.mode = Mode::Search(SearchState::new(
                    self.search_query.clone(),
                    self.current_ticket_id(),
                ));
            }
            Action::ClearSearch => {
                let current_id = self.current_ticket_id();
                self.search_query.clear();
                self.refresh(current_id.as_deref())?;
                self.status = "cleared search".to_string();
                self.mode = Mode::Normal;
            }
            Action::Quit => return Ok(true),
        }
        Ok(false)
    }

    /// Build action state for action menu.
    pub fn build_action_state(&self) -> ActionState {
        let mut items = Vec::new();

        items.push(ActionItem {
            hotkey: 'n',
            label: "New ticket".to_string(),
            enabled: true,
            action: Action::NewTicket,
        });

        if let Some(ticket) = self.tickets.get(self.selected) {
            items.push(ActionItem {
                hotkey: 'a',
                label: "Add note".to_string(),
                enabled: true,
                action: Action::AddNote(ticket.id.as_str().to_string()),
            });
            items.push(ActionItem {
                hotkey: 'e',
                label: "Edit ticket".to_string(),
                enabled: true,
                action: Action::EditTicket(ticket.id.as_str().to_string()),
            });
            items.push(ActionItem {
                hotkey: 'o',
                label: "Edit ticket ($EDITOR)".to_string(),
                enabled: true,
                action: Action::EditTicketEditor(ticket.id.as_str().to_string()),
            });
            items.push(ActionItem {
                hotkey: 'E',
                label: "Edit notes ($EDITOR)".to_string(),
                enabled: true,
                action: Action::EditNotes(ticket.id.as_str().to_string()),
            });
            items.push(ActionItem {
                hotkey: 'p',
                label: "Set priority".to_string(),
                enabled: true,
                action: Action::SetPriority(ticket.id.as_str().to_string()),
            });
            items.push(ActionItem {
                hotkey: 'v',
                label: "Set severity".to_string(),
                enabled: true,
                action: Action::SetSeverity(ticket.id.as_str().to_string()),
            });
            items.push(ActionItem {
                hotkey: 'u',
                label: "Set assignees".to_string(),
                enabled: true,
                action: Action::SetAssignees(ticket.id.as_str().to_string()),
            });

            if matches!(ticket.status, TicketStatus::Closed | TicketStatus::Archived) {
                items.push(ActionItem {
                    hotkey: 'r',
                    label: "Reopen ticket".to_string(),
                    enabled: true,
                    action: Action::Reopen(ticket.id.as_str().to_string()),
                });
            } else {
                items.push(ActionItem {
                    hotkey: 'c',
                    label: "Close ticket".to_string(),
                    enabled: true,
                    action: Action::Close(ticket.id.as_str().to_string()),
                });
            }
        } else {
            items.push(ActionItem {
                hotkey: 'a',
                label: "Add note".to_string(),
                enabled: false,
                action: Action::AddNote(String::new()),
            });
            items.push(ActionItem {
                hotkey: 'e',
                label: "Edit ticket".to_string(),
                enabled: false,
                action: Action::EditTicket(String::new()),
            });
            items.push(ActionItem {
                hotkey: 'o',
                label: "Edit ticket ($EDITOR)".to_string(),
                enabled: false,
                action: Action::EditTicketEditor(String::new()),
            });
            items.push(ActionItem {
                hotkey: 'E',
                label: "Edit notes ($EDITOR)".to_string(),
                enabled: false,
                action: Action::EditNotes(String::new()),
            });
            items.push(ActionItem {
                hotkey: 'p',
                label: "Set priority".to_string(),
                enabled: false,
                action: Action::SetPriority(String::new()),
            });
            items.push(ActionItem {
                hotkey: 'v',
                label: "Set severity".to_string(),
                enabled: false,
                action: Action::SetSeverity(String::new()),
            });
            items.push(ActionItem {
                hotkey: 'u',
                label: "Set assignees".to_string(),
                enabled: false,
                action: Action::SetAssignees(String::new()),
            });
            items.push(ActionItem {
                hotkey: 'c',
                label: "Close ticket".to_string(),
                enabled: false,
                action: Action::Close(String::new()),
            });
        }

        items.push(ActionItem {
            hotkey: '/',
            label: "Search".to_string(),
            enabled: true,
            action: Action::Search,
        });

        if !self.search_query.is_empty() {
            items.push(ActionItem {
                hotkey: 'x',
                label: "Clear search".to_string(),
                enabled: true,
                action: Action::ClearSearch,
            });
        }

        items.push(ActionItem {
            hotkey: 'f',
            label: "Cycle filter".to_string(),
            enabled: true,
            action: Action::ToggleFilter,
        });
        items.push(ActionItem {
            hotkey: 'g',
            label: "Refresh".to_string(),
            enabled: true,
            action: Action::Refresh,
        });
        items.push(ActionItem {
            hotkey: 'q',
            label: "Quit".to_string(),
            enabled: true,
            action: Action::Quit,
        });

        let selected = items.iter().position(|item| item.enabled).unwrap_or(0);

        ActionState { items, selected }
    }

    /// Request close confirmation.
    pub fn request_close(&mut self, id: String) {
        self.mode = Mode::Confirm(ConfirmState {
            prompt: format!("Close {}?", id),
            action: ConfirmAction::Close(id),
        });
    }

    /// Request reopen confirmation.
    pub fn request_reopen(&mut self, id: String) {
        self.mode = Mode::Confirm(ConfirmState {
            prompt: format!("Reopen {}?", id),
            action: ConfirmAction::Reopen(id),
        });
    }
}

/// Check if ticket matches search query.
pub fn ticket_matches_query(ticket: &tik_core::Ticket, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    if contains_ci(ticket.id.as_str(), query) {
        return true;
    }
    if contains_ci(&ticket.title, query) {
        return true;
    }
    if contains_ci(&ticket.summary, query) {
        return true;
    }
    if ticket
        .tags
        .iter()
        .any(|tag| contains_ci(tag.as_str(), query))
    {
        return true;
    }
    if ticket
        .assignees
        .iter()
        .any(|assignee| contains_ci(assignee.as_str(), query))
    {
        return true;
    }
    false
}

/// Case-insensitive contains.
fn contains_ci(haystack: &str, needle: &str) -> bool {
    let needle = needle.to_lowercase();
    haystack.to_lowercase().contains(&needle)
}

/// Insert character at position.
fn insert_char(text: &mut String, index: usize, ch: char) {
    let byte_idx = char_to_byte_index(text, index);
    text.insert(byte_idx, ch);
}

/// Remove character at position.
fn remove_char(text: &mut String, index: usize) {
    if index == 0 {
        return;
    }
    let start = char_to_byte_index(text, index - 1);
    let end = char_to_byte_index(text, index);
    text.replace_range(start..end, "");
}

/// Convert char index to byte index.
fn char_to_byte_index(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
}

/// Resolve actor from environment.
fn resolve_actor() -> String {
    if let Ok(actor) = std::env::var("TIK_ACTOR") {
        if !actor.trim().is_empty() {
            return actor;
        }
    }
    if let Ok(actor) = std::env::var("USER") {
        if !actor.trim().is_empty() {
            return actor;
        }
    }
    "unknown".to_string()
}

/// Parse CSV list.
fn parse_csv_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect()
}

/// Parse priority string.
fn parse_priority(raw: &str) -> Option<Priority> {
    match raw.trim().to_lowercase().as_str() {
        "low" => Some(Priority::Low),
        "medium" => Some(Priority::Medium),
        "high" => Some(Priority::High),
        "critical" => Some(Priority::Critical),
        _ => None,
    }
}

/// Parse severity string.
fn parse_severity(raw: &str) -> Option<Severity> {
    match raw.trim().to_lowercase().as_str() {
        "low" => Some(Severity::Low),
        "normal" => Some(Severity::Normal),
        "high" => Some(Severity::High),
        "critical" => Some(Severity::Critical),
        _ => None,
    }
}

/// Parse status string.
fn parse_status(raw: &str) -> Option<TicketStatus> {
    match raw.trim().to_lowercase().as_str() {
        "open" => Some(TicketStatus::Open),
        "in_progress" => Some(TicketStatus::InProgress),
        "blocked" => Some(TicketStatus::Blocked),
        "closed" => Some(TicketStatus::Closed),
        "archived" => Some(TicketStatus::Archived),
        _ => None,
    }
}

/// Apply edit to ticket.
fn apply_ticket_edit<F>(
    repo: &tik_core::Repo,
    id: &TicketId,
    actor: &str,
    reason: &str,
    mutator: F,
) -> Result<tik_core::Ticket>
where
    F: FnOnce(&mut tik_core::Ticket),
{
    let mut ticket = repo.load_ticket(id)?;
    mutator(&mut ticket);
    let raw = serde_json::to_string_pretty(&ticket)
        .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
    repo.apply_edit(id, &raw, actor, Some(reason))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use tempfile::tempdir;
    use tik_core::{NewMilestone, NewTicket, TicketId};

    #[test]
    fn parse_csv_list_trims_and_dedups_empty() {
        let list = parse_csv_list(" one, two ,, three ,");
        assert_eq!(list, vec!["one", "two", "three"]);
    }

    #[test]
    fn char_helpers_insert_and_remove() {
        let mut text = String::from("ab");
        insert_char(&mut text, 1, 'x');
        assert_eq!(text, "axb");
        remove_char(&mut text, 2);
        assert_eq!(text, "ab");
    }

    #[test]
    fn parse_priority_accepts_known_values() {
        assert!(matches!(parse_priority("low"), Some(Priority::Low)));
        assert!(matches!(parse_priority("MEDIUM"), Some(Priority::Medium)));
        assert!(matches!(parse_priority("High"), Some(Priority::High)));
        assert!(matches!(
            parse_priority("critical"),
            Some(Priority::Critical)
        ));
        assert!(parse_priority("other").is_none());
    }

    #[test]
    fn parse_severity_accepts_known_values() {
        assert!(matches!(parse_severity("low"), Some(Severity::Low)));
        assert!(matches!(parse_severity("NORMAL"), Some(Severity::Normal)));
        assert!(matches!(parse_severity("High"), Some(Severity::High)));
        assert!(matches!(
            parse_severity("critical"),
            Some(Severity::Critical)
        ));
        assert!(parse_severity("other").is_none());
    }

    #[test]
    fn edit_ticket_updates_fields_without_changing_id() {
        let dir = tempdir().expect("temp dir");
        let repo = tik_core::Repo::init(dir.path(), "0.1.0").expect("init repo");
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Original".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "tester",
            )
            .expect("create ticket");

        let mut app = App::new(dir.path().to_path_buf(), Some(repo), false).expect("create app");
        let mut state = InputState::edit_ticket(&ticket);
        state.fields[0].value = "Updated".to_string();
        state.fields[1].value = "Summary".to_string();
        state.fields[2].value = "Desc".to_string();
        state.fields[3].value = "one, two".to_string();
        state.fields[4].value = "alice, bob".to_string();
        state.fields[5].value = "high".to_string();
        state.fields[6].value = "critical".to_string();
        state.fields[7].value = "in_progress".to_string();
        state.fields[8].value = "".to_string();

        let submitted = app.submit_input(&state).expect("submit edit");
        assert!(submitted);

        let updated = app
            .repo
            .as_ref()
            .unwrap()
            .load_ticket(&ticket.id)
            .expect("load updated");
        assert_eq!(updated.id, ticket.id);
        assert_eq!(updated.title, "Updated");
        assert_eq!(updated.summary, "Summary");
        assert_eq!(updated.description, "Desc");
        assert_eq!(updated.tags, vec!["one".to_string(), "two".to_string()]);
        assert_eq!(
            updated.assignees,
            vec!["alice".to_string(), "bob".to_string()]
        );
        assert_eq!(updated.priority, Priority::High);
        assert_eq!(updated.severity, Severity::Critical);
        assert_eq!(updated.status, TicketStatus::InProgress);
    }

    #[test]
    fn enter_focuses_details_panel_from_ticket_list() {
        let dir = tempdir().expect("temp dir");
        let repo = tik_core::Repo::init(dir.path(), "0.1.0").expect("init repo");
        repo.create_ticket(
            NewTicket {
                title: "Ticket".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "tester",
        )
        .expect("create ticket");

        let mut app =
            App::new(dir.path().to_path_buf(), Some(repo), false).expect("create app");

        assert_eq!(app.focused_panel, Panel::TicketList);
        let quit = app
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .expect("handle enter");
        assert!(!quit);
        assert_eq!(app.focused_panel, Panel::Details);
    }

    #[test]
    fn esc_returns_focus_to_ticket_list_from_details_panel() {
        let dir = tempdir().expect("temp dir");
        let repo = tik_core::Repo::init(dir.path(), "0.1.0").expect("init repo");
        repo.create_ticket(
            NewTicket {
                title: "Ticket".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "tester",
        )
        .expect("create ticket");

        let mut app =
            App::new(dir.path().to_path_buf(), Some(repo), false).expect("create app");
        let _ = app
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .expect("handle enter");
        assert_eq!(app.focused_panel, Panel::Details);

        let quit = app
            .handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .expect("handle esc");
        assert!(!quit);
        assert_eq!(app.focused_panel, Panel::TicketList);
    }

    #[test]
    fn parse_status_accepts_known_values() {
        assert!(matches!(parse_status("open"), Some(TicketStatus::Open)));
        assert!(matches!(
            parse_status("in_progress"),
            Some(TicketStatus::InProgress)
        ));
        assert!(parse_status("unknown").is_none());
    }

    #[test]
    fn selected_ids_sorted_returns_ordered_ids() {
        let dir = tempdir().unwrap();
        let mut app = App::new(dir.path().to_path_buf(), None, true).unwrap();
        app.selected_ids.insert("T-2".to_string());
        app.selected_ids.insert("T-1".to_string());
        let ids = app.selected_ids_sorted();
        assert_eq!(ids, vec!["T-1".to_string(), "T-2".to_string()]);
    }

    #[test]
    fn bulk_updates_apply_to_selected_tickets() {
        let dir = tempdir().unwrap();
        let repo = tik_core::Repo::init(dir.path(), "0.1.0-test").unwrap();
        let t1 = repo
            .create_ticket(
                NewTicket {
                    title: "One".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "tester",
            )
            .unwrap();
        let t2 = repo
            .create_ticket(
                NewTicket {
                    title: "Two".to_string(),
                    summary: None,
                    description: None,
                    tags: vec![],
                },
                "tester",
            )
            .unwrap();
        let milestone = repo
            .create_milestone(
                NewMilestone {
                    title: "Milestone".to_string(),
                    description: None,
                    due_at: None,
                    tags: vec![],
                },
                "tester",
            )
            .unwrap();

        let mut app = App::new(dir.path().to_path_buf(), Some(repo), true).unwrap();
        let ids = vec![t1.id.as_str().to_string(), t2.id.as_str().to_string()];
        app.bulk_set_priority(ids.clone(), Priority::High).unwrap();
        app.bulk_set_severity(ids.clone(), Severity::Critical).unwrap();
        app.bulk_set_status(ids.clone(), TicketStatus::Blocked).unwrap();
        app.bulk_set_assignees(ids.clone(), vec!["ada".to_string()])
            .unwrap();
        app.bulk_set_milestone(ids.clone(), Some(milestone.id.as_str().to_string()))
            .unwrap();

        let repo = app.repo.as_ref().unwrap();
        for id in ids {
            let ticket = repo.load_ticket(&TicketId::parse(&id).unwrap()).unwrap();
            assert_eq!(ticket.priority, Priority::High);
            assert_eq!(ticket.severity, Severity::Critical);
            assert_eq!(ticket.status, TicketStatus::Blocked);
            assert_eq!(ticket.assignees, vec!["ada".to_string()]);
            assert_eq!(
                ticket.milestone_id.as_ref().unwrap().as_str(),
                milestone.id.as_str()
            );
        }
    }

    #[test]
    fn perform_confirm_action_initializes_repo() {
        let dir = tempdir().unwrap();
        let mut app = App::new(dir.path().to_path_buf(), None, true).unwrap();
        app.perform_confirm_action(ConfirmAction::InitRepo {
            setup_claude: false,
            setup_agents: false,
        })
        .unwrap();
        assert!(app.repo.is_some());
        assert_eq!(app.status, "initialized repo");
    }
}
