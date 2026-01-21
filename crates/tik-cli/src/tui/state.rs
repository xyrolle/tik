//! TUI state types and enums.

use tik_core::{Event, Milestone, TicketStatus};

/// Active panel in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    TicketList,
    Details,
}

impl Panel {
    pub fn next(self) -> Self {
        match self {
            Panel::TicketList => Panel::Details,
            Panel::Details => Panel::TicketList,
        }
    }
}

/// Filter for ticket list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TicketFilter {
    All,
    Open,
    InProgress,
    Blocked,
    Closed,
    Archived,
}

impl TicketFilter {
    pub fn next(self) -> Self {
        match self {
            TicketFilter::All => TicketFilter::Open,
            TicketFilter::Open => TicketFilter::InProgress,
            TicketFilter::InProgress => TicketFilter::Blocked,
            TicketFilter::Blocked => TicketFilter::Closed,
            TicketFilter::Closed => TicketFilter::Archived,
            TicketFilter::Archived => TicketFilter::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TicketFilter::All => "all",
            TicketFilter::Open => "open",
            TicketFilter::InProgress => "in_progress",
            TicketFilter::Blocked => "blocked",
            TicketFilter::Closed => "closed",
            TicketFilter::Archived => "archived",
        }
    }

    pub fn status(self) -> Option<TicketStatus> {
        match self {
            TicketFilter::All => None,
            TicketFilter::Open => Some(TicketStatus::Open),
            TicketFilter::InProgress => Some(TicketStatus::InProgress),
            TicketFilter::Blocked => Some(TicketStatus::Blocked),
            TicketFilter::Closed => Some(TicketStatus::Closed),
            TicketFilter::Archived => Some(TicketStatus::Archived),
        }
    }
}

/// Tab in the details panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailsTab {
    Info,
    Notes,
    History,
    Relations,
}

impl DetailsTab {
    pub fn next(self) -> Self {
        match self {
            DetailsTab::Info => DetailsTab::Notes,
            DetailsTab::Notes => DetailsTab::History,
            DetailsTab::History => DetailsTab::Relations,
            DetailsTab::Relations => DetailsTab::Info,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            DetailsTab::Info => DetailsTab::Relations,
            DetailsTab::Notes => DetailsTab::Info,
            DetailsTab::History => DetailsTab::Notes,
            DetailsTab::Relations => DetailsTab::History,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DetailsTab::Info => "Info",
            DetailsTab::Notes => "Notes",
            DetailsTab::History => "History",
            DetailsTab::Relations => "Relations",
        }
    }

    pub fn index(self) -> usize {
        match self {
            DetailsTab::Info => 0,
            DetailsTab::Notes => 1,
            DetailsTab::History => 2,
            DetailsTab::Relations => 3,
        }
    }
}

/// Current mode of the TUI.
#[derive(Debug)]
pub enum Mode {
    Normal,
    Input(InputState),
    Confirm(ConfirmState),
    Action(ActionState),
    Search(SearchState),
    Select(SelectState),
    Help,
    MilestoneMenu(MilestoneMenuState),
    TagEdit(TagEditState),
}

/// Type of input being collected.
#[derive(Debug)]
pub enum InputKind {
    NewTicket,
    AddNote(String),
    CloseReason(String),
    ReopenReason(String),
    SetAssignees(String),
}

/// A single input field.
#[derive(Debug)]
pub struct InputField {
    pub label: &'static str,
    pub value: String,
    pub required: bool,
}

/// State for input mode.
#[derive(Debug)]
pub struct InputState {
    pub title: &'static str,
    pub kind: InputKind,
    pub fields: Vec<InputField>,
    pub current: usize,
    pub cursor: usize,
}

impl InputState {
    pub fn new_ticket() -> Self {
        Self {
            title: "New Ticket",
            kind: InputKind::NewTicket,
            fields: vec![
                InputField {
                    label: "Title",
                    value: String::new(),
                    required: true,
                },
                InputField {
                    label: "Summary",
                    value: String::new(),
                    required: false,
                },
                InputField {
                    label: "Description",
                    value: String::new(),
                    required: false,
                },
                InputField {
                    label: "Tags (comma-separated)",
                    value: String::new(),
                    required: false,
                },
            ],
            current: 0,
            cursor: 0,
        }
    }

    pub fn add_note(id: String) -> Self {
        Self {
            title: "Add Note",
            kind: InputKind::AddNote(id),
            fields: vec![InputField {
                label: "Note",
                value: String::new(),
                required: true,
            }],
            current: 0,
            cursor: 0,
        }
    }

    pub fn close_reason(id: String) -> Self {
        Self {
            title: "Close Ticket",
            kind: InputKind::CloseReason(id),
            fields: vec![InputField {
                label: "Reason (optional)",
                value: String::new(),
                required: false,
            }],
            current: 0,
            cursor: 0,
        }
    }

    pub fn reopen_reason(id: String) -> Self {
        Self {
            title: "Reopen Ticket",
            kind: InputKind::ReopenReason(id),
            fields: vec![InputField {
                label: "Reason (optional)",
                value: String::new(),
                required: false,
            }],
            current: 0,
            cursor: 0,
        }
    }

    pub fn set_assignees(id: String) -> Self {
        Self {
            title: "Set Assignees",
            kind: InputKind::SetAssignees(id),
            fields: vec![InputField {
                label: "Assignees (comma-separated, empty clears)",
                value: String::new(),
                required: false,
            }],
            current: 0,
            cursor: 0,
        }
    }
}

/// Action to confirm.
#[derive(Debug)]
pub enum ConfirmAction {
    InitRepo,
    Close(String),
    Reopen(String),
}

/// State for confirmation mode.
#[derive(Debug)]
pub struct ConfirmState {
    pub prompt: String,
    pub action: ConfirmAction,
}

/// Action that can be performed.
#[derive(Debug, Clone)]
pub enum Action {
    NewTicket,
    AddNote(String),
    EditTicket(String),
    EditNotes(String),
    Close(String),
    Reopen(String),
    SetPriority(String),
    SetSeverity(String),
    SetAssignees(String),
    ToggleFilter,
    Refresh,
    Search,
    ClearSearch,
    Quit,
}

/// An item in the action menu.
#[derive(Debug, Clone)]
pub struct ActionItem {
    pub hotkey: char,
    pub label: String,
    pub enabled: bool,
    pub action: Action,
}

/// State for action menu mode.
#[derive(Debug)]
pub struct ActionState {
    pub items: Vec<ActionItem>,
    pub selected: usize,
}

/// State for search mode.
#[derive(Debug)]
pub struct SearchState {
    pub query: String,
    pub cursor: usize,
    pub original: String,
    pub original_selected: Option<String>,
    pub match_indices: Vec<usize>,
    pub current_match: usize,
}

impl SearchState {
    pub fn new(query: String, selected: Option<String>) -> Self {
        let cursor = query.chars().count();
        Self {
            original: query.clone(),
            query,
            cursor,
            original_selected: selected,
            match_indices: Vec::new(),
            current_match: 0,
        }
    }
}

/// Option in a select menu.
#[derive(Debug, Clone)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
    pub hotkey: Option<char>,
}

/// Type of selection being made.
#[derive(Debug, Clone)]
pub enum SelectKind {
    Priority(String),
    Severity(String),
    Status(String),
    Filter,
}

/// State for selection menu mode.
#[derive(Debug)]
pub struct SelectState {
    pub title: String,
    pub kind: SelectKind,
    pub options: Vec<SelectOption>,
    pub selected: usize,
}

impl SelectState {
    pub fn priority(ticket_id: String) -> Self {
        Self {
            title: "Select Priority".to_string(),
            kind: SelectKind::Priority(ticket_id),
            options: vec![
                SelectOption {
                    label: "Low".to_string(),
                    value: "low".to_string(),
                    hotkey: Some('l'),
                },
                SelectOption {
                    label: "Medium".to_string(),
                    value: "medium".to_string(),
                    hotkey: Some('m'),
                },
                SelectOption {
                    label: "High".to_string(),
                    value: "high".to_string(),
                    hotkey: Some('h'),
                },
                SelectOption {
                    label: "Critical".to_string(),
                    value: "critical".to_string(),
                    hotkey: Some('c'),
                },
            ],
            selected: 0,
        }
    }

    pub fn severity(ticket_id: String) -> Self {
        Self {
            title: "Select Severity".to_string(),
            kind: SelectKind::Severity(ticket_id),
            options: vec![
                SelectOption {
                    label: "Low".to_string(),
                    value: "low".to_string(),
                    hotkey: Some('l'),
                },
                SelectOption {
                    label: "Normal".to_string(),
                    value: "normal".to_string(),
                    hotkey: Some('n'),
                },
                SelectOption {
                    label: "High".to_string(),
                    value: "high".to_string(),
                    hotkey: Some('h'),
                },
                SelectOption {
                    label: "Critical".to_string(),
                    value: "critical".to_string(),
                    hotkey: Some('c'),
                },
            ],
            selected: 0,
        }
    }

    pub fn status(ticket_id: String) -> Self {
        Self {
            title: "Select Status".to_string(),
            kind: SelectKind::Status(ticket_id),
            options: vec![
                SelectOption {
                    label: "Open".to_string(),
                    value: "open".to_string(),
                    hotkey: Some('o'),
                },
                SelectOption {
                    label: "In Progress".to_string(),
                    value: "in_progress".to_string(),
                    hotkey: Some('i'),
                },
                SelectOption {
                    label: "Blocked".to_string(),
                    value: "blocked".to_string(),
                    hotkey: Some('b'),
                },
                SelectOption {
                    label: "Closed".to_string(),
                    value: "closed".to_string(),
                    hotkey: Some('c'),
                },
                SelectOption {
                    label: "Archived".to_string(),
                    value: "archived".to_string(),
                    hotkey: Some('a'),
                },
            ],
            selected: 0,
        }
    }

    pub fn filter() -> Self {
        Self {
            title: "Select Filter".to_string(),
            kind: SelectKind::Filter,
            options: vec![
                SelectOption {
                    label: "All".to_string(),
                    value: "all".to_string(),
                    hotkey: Some('a'),
                },
                SelectOption {
                    label: "Open".to_string(),
                    value: "open".to_string(),
                    hotkey: Some('o'),
                },
                SelectOption {
                    label: "In Progress".to_string(),
                    value: "in_progress".to_string(),
                    hotkey: Some('i'),
                },
                SelectOption {
                    label: "Blocked".to_string(),
                    value: "blocked".to_string(),
                    hotkey: Some('b'),
                },
                SelectOption {
                    label: "Closed".to_string(),
                    value: "closed".to_string(),
                    hotkey: Some('c'),
                },
                SelectOption {
                    label: "Archived".to_string(),
                    value: "archived".to_string(),
                    hotkey: Some('r'),
                },
            ],
            selected: 0,
        }
    }
}

/// State for milestone menu.
#[derive(Debug)]
pub struct MilestoneMenuState {
    pub ticket_id: String,
    pub milestones: Vec<Milestone>,
    pub selected: usize,
}

/// State for tag edit mode.
#[derive(Debug)]
pub struct TagEditState {
    pub ticket_id: String,
    /// Current tags on the ticket.
    pub tags: Vec<String>,
    /// Input buffer for new tag.
    pub input: String,
    /// Cursor position in input.
    pub cursor: usize,
    /// Original tags for cancel restore.
    pub original_tags: Vec<String>,
    /// All known tags for autocomplete.
    pub all_tags: Vec<String>,
    /// Autocomplete suggestions.
    pub suggestions: Vec<String>,
    /// Selected suggestion index (if any).
    pub suggestion_idx: Option<usize>,
}

impl TagEditState {
    pub fn new(ticket_id: String, current_tags: Vec<String>, all_tags: Vec<String>) -> Self {
        Self {
            ticket_id,
            original_tags: current_tags.clone(),
            tags: current_tags,
            input: String::new(),
            cursor: 0,
            all_tags,
            suggestions: Vec::new(),
            suggestion_idx: None,
        }
    }

    /// Update autocomplete suggestions based on current input.
    pub fn update_suggestions(&mut self) {
        let input_lower = self.input.to_lowercase();
        if input_lower.is_empty() {
            self.suggestions.clear();
            self.suggestion_idx = None;
            return;
        }

        self.suggestions = self
            .all_tags
            .iter()
            .filter(|tag| {
                let tag_lower = tag.to_lowercase();
                tag_lower.starts_with(&input_lower) && !self.tags.contains(tag)
            })
            .take(5)
            .cloned()
            .collect();

        if self.suggestions.is_empty() {
            self.suggestion_idx = None;
        } else if self.suggestion_idx.is_none() {
            self.suggestion_idx = Some(0);
        } else if let Some(idx) = self.suggestion_idx {
            if idx >= self.suggestions.len() {
                self.suggestion_idx = Some(self.suggestions.len().saturating_sub(1));
            }
        }
    }

    /// Add the current input as a tag (normalized).
    pub fn add_current_tag(&mut self) -> bool {
        let tag = normalize_tag_input(&self.input);
        if tag.is_empty() || self.tags.contains(&tag) {
            return false;
        }
        self.tags.push(tag);
        self.input.clear();
        self.cursor = 0;
        self.suggestions.clear();
        self.suggestion_idx = None;
        true
    }

    /// Add the selected suggestion as a tag.
    pub fn add_selected_suggestion(&mut self) -> bool {
        if let Some(idx) = self.suggestion_idx {
            if let Some(tag) = self.suggestions.get(idx).cloned() {
                if !self.tags.contains(&tag) {
                    self.tags.push(tag);
                    self.input.clear();
                    self.cursor = 0;
                    self.suggestions.clear();
                    self.suggestion_idx = None;
                    return true;
                }
            }
        }
        false
    }

    /// Remove the last tag.
    pub fn remove_last_tag(&mut self) -> bool {
        if self.input.is_empty() && !self.tags.is_empty() {
            self.tags.pop();
            return true;
        }
        false
    }

    /// Cycle through suggestions.
    pub fn next_suggestion(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.suggestion_idx = Some(match self.suggestion_idx {
            Some(idx) => (idx + 1) % self.suggestions.len(),
            None => 0,
        });
    }

    /// Cycle through suggestions backwards.
    pub fn prev_suggestion(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.suggestion_idx = Some(match self.suggestion_idx {
            Some(idx) => {
                if idx == 0 {
                    self.suggestions.len() - 1
                } else {
                    idx - 1
                }
            }
            None => 0,
        });
    }
}

/// Normalize a tag input to slug format.
fn normalize_tag_input(input: &str) -> String {
    input
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Cached details for the selected ticket.
#[derive(Debug, Default)]
pub struct TicketDetails {
    pub ticket_id: Option<String>,
    pub notes: Option<String>,
    pub events: Vec<Event>,
    pub scroll_offset: usize,
}

impl TicketDetails {
    pub fn clear(&mut self) {
        self.ticket_id = None;
        self.notes = None;
        self.events.clear();
        self.scroll_offset = 0;
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }
}

/// Pending vim command (for two-key sequences like `gg`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingKey {
    None,
    G,
}

/// Target file for editor launch.
#[derive(Debug, Clone)]
pub enum EditorTarget {
    /// Edit ticket.json
    Ticket(String),
    /// Edit notes.md
    Notes(String),
}

/// Pending editor action to execute after key handling.
#[derive(Debug, Clone, Default)]
pub struct PendingEditor {
    pub target: Option<EditorTarget>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_cycles() {
        let mut filter = TicketFilter::All;
        filter = filter.next();
        assert!(matches!(filter, TicketFilter::Open));
        filter = filter.next();
        assert!(matches!(filter, TicketFilter::InProgress));
        filter = filter.next();
        assert!(matches!(filter, TicketFilter::Blocked));
        filter = filter.next();
        assert!(matches!(filter, TicketFilter::Closed));
        filter = filter.next();
        assert!(matches!(filter, TicketFilter::Archived));
        filter = filter.next();
        assert!(matches!(filter, TicketFilter::All));
    }

    #[test]
    fn panel_cycles() {
        let panel = Panel::TicketList;
        assert_eq!(panel.next(), Panel::Details);
        assert_eq!(panel.next().next(), Panel::TicketList);
    }

    #[test]
    fn details_tab_cycles() {
        let tab = DetailsTab::Info;
        assert_eq!(tab.next(), DetailsTab::Notes);
        assert_eq!(tab.next().next(), DetailsTab::History);
        assert_eq!(tab.prev(), DetailsTab::Relations);
    }
}
