//! TUI input handling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tik_core::{MilestoneId, Priority, Result, Severity, TicketId, TicketStatus, TikError};
#[allow(unused_imports)]
use tik_core::TicketStatus as _;

use super::app::App;
use super::state::{
    Action, ActionItem, ActionState, ConfirmAction, ConfirmState, DetailsTab, InputKind,
    InputState, MilestoneMenuState, Mode, Panel, PendingKey, SearchState, SelectKind, SelectState,
    TagEditState, TicketFilter,
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
            Mode::Action(_) => return self.handle_action_mode(key),
            Mode::Search(_) => return self.handle_search_mode(key),
            Mode::Select(_) => return self.handle_select_mode(key),
            Mode::Help => return self.handle_help_mode(key),
            Mode::MilestoneMenu(_) => return self.handle_milestone_mode(key),
            Mode::TagEdit(_) => return self.handle_tag_edit_mode(key),
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

            // Panel switching
            KeyCode::Tab => {
                self.focused_panel = self.focused_panel.next();
                self.ticket_details.reset_scroll();
            }
            KeyCode::Enter => {
                if self.focused_panel == Panel::TicketList && !self.tickets.is_empty() {
                    self.focused_panel = Panel::Details;
                    self.ticket_details.reset_scroll();
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
                    self.details_tab = self.details_tab.prev();
                    self.ticket_details.reset_scroll();
                }
            }
            KeyCode::Char(']') | KeyCode::Right => {
                if self.focused_panel == Panel::Details {
                    self.details_tab = self.details_tab.next();
                    self.ticket_details.reset_scroll();
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
                    self.bulk_close()?;
                } else if let Some(ticket) = self.tickets.get(self.selected) {
                    self.request_close(ticket.id.as_str().to_string());
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Reopen ticket(s)
            KeyCode::Char('r') => {
                if self.selection_mode && !self.selected_ids.is_empty() {
                    self.bulk_reopen()?;
                } else if let Some(ticket) = self.tickets.get(self.selected) {
                    self.request_reopen(ticket.id.as_str().to_string());
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Set priority (selection menu)
            KeyCode::Char('p') => {
                if let Some(ticket) = self.tickets.get(self.selected) {
                    self.mode = Mode::Select(SelectState::priority(ticket.id.as_str().to_string()));
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Set severity (selection menu)
            KeyCode::Char('v') => {
                if let Some(ticket) = self.tickets.get(self.selected) {
                    self.mode = Mode::Select(SelectState::severity(ticket.id.as_str().to_string()));
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Set status (selection menu)
            KeyCode::Char('s') => {
                if let Some(ticket) = self.tickets.get(self.selected) {
                    self.mode = Mode::Select(SelectState::status(ticket.id.as_str().to_string()));
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Set assignees
            KeyCode::Char('U') => {
                if let Some(ticket) = self.tickets.get(self.selected) {
                    self.mode =
                        Mode::Input(InputState::set_assignees(ticket.id.as_str().to_string()));
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }

            // Set milestone
            KeyCode::Char('M') => {
                if let Some(ticket) = self.tickets.get(self.selected) {
                    self.open_milestone_menu(ticket.id.as_str().to_string())?;
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

            // Edit in $EDITOR
            KeyCode::Char('e') => {
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

            _ => {}
        }
        Ok(false)
    }

    /// Handle key in uninitialized state.
    fn handle_uninitialized_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('i') => {
                self.mode = Mode::Confirm(ConfirmState {
                    prompt: "Initialize .tik repo here?".to_string(),
                    action: ConfirmAction::InitRepo,
                });
            }
            KeyCode::Char('q') => return Ok(true),
            _ => {}
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
                return self.set_ticket_milestone(&state.ticket_id, None);
            }
            KeyCode::Enter => {
                if state.selected == 0 {
                    // Clear milestone
                    return self.set_ticket_milestone(&state.ticket_id, None);
                } else {
                    // Set milestone
                    let milestone = &state.milestones[state.selected - 1];
                    return self
                        .set_ticket_milestone(&state.ticket_id, Some(milestone.id.as_str()));
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
            KeyCode::Tab => {
                if state.current + 1 < state.fields.len() {
                    state.current += 1;
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
                self.mode = Mode::Normal;
                return Ok(false);
            }
            self.mode = Mode::Input(state);
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

    /// Open milestone menu.
    fn open_milestone_menu(&mut self, ticket_id: String) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };

        let milestones = repo.list_milestones(None)?;
        self.mode = Mode::MilestoneMenu(MilestoneMenuState {
            ticket_id,
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
    fn bulk_close(&mut self) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };

        let actor = resolve_actor();
        let ids: Vec<String> = self.selected_ids.iter().cloned().collect();
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
    fn bulk_reopen(&mut self) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.status = "repo not initialized".to_string();
            return Ok(());
        };

        let actor = resolve_actor();
        let ids: Vec<String> = self.selected_ids.iter().cloned().collect();
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

    /// Perform confirmed action.
    fn perform_confirm_action(&mut self, action: ConfirmAction) -> Result<()> {
        match action {
            ConfirmAction::InitRepo => {
                let repo = tik_core::Repo::init(&self.root, env!("CARGO_PKG_VERSION"))?;
                self.repo = Some(repo);
                self.refresh(None)?;
                self.status = "initialized repo".to_string();
            }
            ConfirmAction::Close(id) => {
                self.mode = Mode::Input(InputState::close_reason(id));
            }
            ConfirmAction::Reopen(id) => {
                self.mode = Mode::Input(InputState::reopen_reason(id));
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
                    self.mode = Mode::Select(SelectState::priority(id));
                }
            }
            Action::SetSeverity(id) => {
                if id.is_empty() {
                    self.status = "no ticket selected".to_string();
                } else {
                    self.mode = Mode::Select(SelectState::severity(id));
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
                label: "Edit ticket ($EDITOR)".to_string(),
                enabled: true,
                action: Action::EditTicket(ticket.id.as_str().to_string()),
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
                label: "Edit ticket ($EDITOR)".to_string(),
                enabled: false,
                action: Action::EditTicket(String::new()),
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
    use tik_core::NewTicket;

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
}
