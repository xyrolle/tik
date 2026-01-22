//! TUI state types and enums.

use std::collections::{HashMap, HashSet};

use tik_core::{
    Event, Graph, GraphNode, Milestone, Priority, RelationType, Severity, Ticket, TicketStatus,
};

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
    Diff,
}

impl DetailsTab {
    pub fn next(self) -> Self {
        match self {
            DetailsTab::Info => DetailsTab::Notes,
            DetailsTab::Notes => DetailsTab::History,
            DetailsTab::History => DetailsTab::Relations,
            DetailsTab::Relations => DetailsTab::Diff,
            DetailsTab::Diff => DetailsTab::Info,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            DetailsTab::Info => DetailsTab::Diff,
            DetailsTab::Notes => DetailsTab::Info,
            DetailsTab::History => DetailsTab::Notes,
            DetailsTab::Relations => DetailsTab::History,
            DetailsTab::Diff => DetailsTab::Relations,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DetailsTab::Info => "Info",
            DetailsTab::Notes => "Notes",
            DetailsTab::History => "History",
            DetailsTab::Relations => "Relations",
            DetailsTab::Diff => "Diff",
        }
    }

    pub fn index(self) -> usize {
        match self {
            DetailsTab::Info => 0,
            DetailsTab::Notes => 1,
            DetailsTab::History => 2,
            DetailsTab::Relations => 3,
            DetailsTab::Diff => 4,
        }
    }
}

/// A snapshot version for diff comparison.
#[derive(Debug, Clone)]
pub struct DiffSnapshot {
    pub timestamp: String,
    pub actor: String,
    pub content: String,
}

/// A line in a diff view.
#[derive(Debug, Clone)]
pub enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
    Header(String),
}

/// State for graph view overlay.
#[derive(Debug)]
pub struct GraphState {
    /// The graph data.
    pub graph: Graph,
    /// Rendered ASCII lines for display.
    pub rendered_lines: Vec<GraphLine>,
    /// Scroll offset for the graph view.
    pub scroll_offset: usize,
    /// Currently focused/highlighted node index.
    pub focused_node: usize,
}

/// A rendered line in the graph view.
#[derive(Debug, Clone)]
pub struct GraphLine {
    pub content: String,
    pub node_id: Option<String>,
    pub is_edge: bool,
}

impl GraphState {
    pub fn new(graph: Graph, root_id: Option<String>) -> Self {
        let rendered_lines = Self::render_graph_lines(&graph, &root_id);
        Self {
            graph,
            rendered_lines,
            scroll_offset: 0,
            focused_node: 0,
        }
    }

    /// Render the graph as ASCII art lines.
    fn render_graph_lines(graph: &Graph, root_id: &Option<String>) -> Vec<GraphLine> {
        let mut lines = Vec::new();

        if graph.nodes.is_empty() {
            lines.push(GraphLine {
                content: "No tickets with relations found.".to_string(),
                node_id: None,
                is_edge: false,
            });
            return lines;
        }

        // Build adjacency list with owned strings
        let mut outgoing: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let mut incoming: HashMap<String, Vec<(String, String)>> = HashMap::new();

        for edge in &graph.edges {
            outgoing
                .entry(edge.from.clone())
                .or_default()
                .push((edge.to.clone(), edge.relation.clone()));
            incoming
                .entry(edge.to.clone())
                .or_default()
                .push((edge.from.clone(), edge.relation.clone()));
        }

        // Build node map for quick lookup
        let node_map: HashMap<String, &GraphNode> = graph
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n))
            .collect();

        // Find root nodes
        let root_ids: Vec<String> = if let Some(ref rid) = root_id {
            if node_map.contains_key(rid) {
                vec![rid.clone()]
            } else {
                vec![]
            }
        } else {
            graph
                .nodes
                .iter()
                .filter(|n| incoming.get(&n.id).map(|v| v.is_empty()).unwrap_or(true))
                .map(|n| n.id.clone())
                .collect()
        };

        // If no roots found, use all nodes
        let root_ids: Vec<String> = if root_ids.is_empty() {
            graph.nodes.iter().map(|n| n.id.clone()).collect()
        } else {
            root_ids
        };

        // Render each root and its descendants
        let mut visited: HashSet<String> = HashSet::new();

        for rid in &root_ids {
            if let Some(node) = node_map.get(rid) {
                Self::render_node_recursive(
                    node,
                    &outgoing,
                    &node_map,
                    &mut visited,
                    &mut lines,
                    0,
                    "",
                );
            }
        }

        // Add any unvisited nodes
        for node in &graph.nodes {
            if !visited.contains(&node.id) {
                lines.push(GraphLine {
                    content: String::new(),
                    node_id: None,
                    is_edge: false,
                });
                Self::render_node_recursive(
                    node,
                    &outgoing,
                    &node_map,
                    &mut visited,
                    &mut lines,
                    0,
                    "",
                );
            }
        }

        lines
    }

    fn render_node_recursive(
        node: &GraphNode,
        outgoing: &HashMap<String, Vec<(String, String)>>,
        node_map: &HashMap<String, &GraphNode>,
        visited: &mut HashSet<String>,
        lines: &mut Vec<GraphLine>,
        depth: usize,
        prefix: &str,
    ) {
        if visited.contains(&node.id) {
            // Circular reference indicator
            let indent = "  ".repeat(depth);
            lines.push(GraphLine {
                content: format!("{}↺ {} (cycle)", indent, short_id(&node.id)),
                node_id: Some(node.id.clone()),
                is_edge: false,
            });
            return;
        }
        visited.insert(node.id.clone());

        // Render the node
        let status_symbol = match node.status.as_str() {
            "open" => "○",
            "in_progress" => "◐",
            "blocked" => "⊘",
            "closed" => "●",
            "archived" => "▣",
            _ => "?",
        };

        let indent = if depth == 0 {
            String::new()
        } else {
            format!("{}├─ ", prefix)
        };

        let title = if node.title.len() > 40 {
            format!("{}...", &node.title[..37])
        } else {
            node.title.clone()
        };

        lines.push(GraphLine {
            content: format!(
                "{}{} {} {}",
                indent,
                status_symbol,
                short_id(&node.id),
                title
            ),
            node_id: Some(node.id.clone()),
            is_edge: false,
        });

        // Get children
        if let Some(children) = outgoing.get(&node.id) {
            let child_count = children.len();
            for (i, (child_id, relation)) in children.iter().enumerate() {
                let is_last = i == child_count - 1;
                let child_prefix = if depth == 0 {
                    if is_last { "  " } else { "│ " }.to_string()
                } else {
                    format!("{}{}  ", prefix, if is_last { "  " } else { "│ " })
                };

                // Show the relation type
                let rel_indent = if depth == 0 {
                    "│".to_string()
                } else {
                    format!("{}│", prefix)
                };
                lines.push(GraphLine {
                    content: format!("{}   [{}]", rel_indent, relation),
                    node_id: None,
                    is_edge: true,
                });

                // Find and render the child node
                if let Some(child_node) = node_map.get(child_id) {
                    Self::render_node_recursive(
                        child_node,
                        outgoing,
                        node_map,
                        visited,
                        lines,
                        depth + 1,
                        &child_prefix,
                    );
                }
            }
        }
    }

    /// Move focus to next node.
    pub fn next_node(&mut self) {
        let node_count = self
            .rendered_lines
            .iter()
            .filter(|l| l.node_id.is_some())
            .count();
        if node_count > 0 && self.focused_node + 1 < node_count {
            self.focused_node += 1;
        }
    }

    /// Move focus to previous node.
    pub fn prev_node(&mut self) {
        if self.focused_node > 0 {
            self.focused_node -= 1;
        }
    }

    /// Scroll down.
    pub fn scroll_down(&mut self) {
        self.scroll_offset += 1;
    }

    /// Scroll up.
    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    /// Get the focused node ID.
    pub fn focused_node_id(&self) -> Option<&str> {
        let mut count = 0;
        for line in &self.rendered_lines {
            if line.node_id.is_some() {
                if count == self.focused_node {
                    return line.node_id.as_deref();
                }
                count += 1;
            }
        }
        None
    }
}

/// Get a short version of a ticket ID.
fn short_id(id: &str) -> &str {
    // T-01KFGFZN1W0WM6T4586T8FPV46 -> T-01KF...PV46
    if id.len() > 15 {
        id
    } else {
        id
    }
}

/// Diff state for comparing ticket versions.
#[derive(Debug, Default)]
pub struct DiffState {
    /// List of snapshots (ticket versions) available.
    pub snapshots: Vec<DiffSnapshot>,
    /// Index of the "before" version (older).
    pub before_idx: usize,
    /// Index of the "after" version (newer).
    pub after_idx: usize,
    /// Computed diff lines.
    pub diff_lines: Vec<DiffLine>,
}

impl DiffState {
    pub fn clear(&mut self) {
        self.snapshots.clear();
        self.before_idx = 0;
        self.after_idx = 0;
        self.diff_lines.clear();
    }

    /// Select next "after" version (newer).
    pub fn next_after(&mut self) {
        if self.after_idx + 1 < self.snapshots.len() {
            self.after_idx += 1;
            if self.before_idx >= self.after_idx {
                self.before_idx = self.after_idx.saturating_sub(1);
            }
        }
    }

    /// Select previous "after" version (older).
    pub fn prev_after(&mut self) {
        if self.after_idx > 1 {
            self.after_idx -= 1;
            if self.before_idx >= self.after_idx {
                self.before_idx = self.after_idx.saturating_sub(1);
            }
        }
    }
}

/// Current mode of the TUI.
#[derive(Debug)]
pub enum Mode {
    Normal,
    Input(InputState),
    Confirm(ConfirmState),
    Init(InitState),
    Action(ActionState),
    Search(SearchState),
    Select(SelectState),
    Help,
    MilestoneMenu(MilestoneMenuState),
    TagEdit(TagEditState),
    RelationEdit(RelationEditState),
    GraphView(GraphState),
}

/// Type of input being collected.
#[derive(Debug)]
pub enum InputKind {
    NewTicket,
    AddNote(String),
    EditTicket(String),
    CloseReason(String),
    ReopenReason(String),
    SetAssignees(String),
    SetAssigneesBulk(Vec<String>),
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

    pub fn edit_ticket(ticket: &Ticket) -> Self {
        let tags = if ticket.tags.is_empty() {
            String::new()
        } else {
            ticket.tags.join(", ")
        };
        let assignees = if ticket.assignees.is_empty() {
            String::new()
        } else {
            ticket.assignees.join(", ")
        };
        let milestone = ticket
            .milestone_id
            .as_ref()
            .map(|id| id.as_str().to_string())
            .unwrap_or_default();

        Self {
            title: "Edit Ticket",
            kind: InputKind::EditTicket(ticket.id.as_str().to_string()),
            fields: vec![
                InputField {
                    label: "Title",
                    value: ticket.title.clone(),
                    required: true,
                },
                InputField {
                    label: "Summary",
                    value: ticket.summary.clone(),
                    required: false,
                },
                InputField {
                    label: "Description",
                    value: ticket.description.clone(),
                    required: false,
                },
                InputField {
                    label: "Tags (comma-separated)",
                    value: tags,
                    required: false,
                },
                InputField {
                    label: "Assignees (comma-separated, empty clears)",
                    value: assignees,
                    required: false,
                },
                InputField {
                    label: "Priority (low|medium|high|critical)",
                    value: ticket.priority.as_str().to_string(),
                    required: true,
                },
                InputField {
                    label: "Severity (low|normal|high|critical)",
                    value: ticket.severity.as_str().to_string(),
                    required: true,
                },
                InputField {
                    label: "Status (open|in_progress|blocked|closed|archived)",
                    value: ticket.status.as_str().to_string(),
                    required: true,
                },
                InputField {
                    label: "Milestone (id or empty clears)",
                    value: milestone,
                    required: false,
                },
            ],
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

    pub fn set_assignees_bulk(ids: Vec<String>) -> Self {
        Self {
            title: "Set Assignees (Bulk)",
            kind: InputKind::SetAssigneesBulk(ids),
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

/// Which checkbox is focused in the init modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InitFocus {
    #[default]
    Claude,
    Agents,
}

impl InitFocus {
    pub fn next(self) -> Self {
        match self {
            InitFocus::Claude => InitFocus::Agents,
            InitFocus::Agents => InitFocus::Claude,
        }
    }
}

/// State for repo initialization modal.
#[derive(Debug)]
pub struct InitState {
    /// Whether to set up Claude Code integration (CLAUDE.md + .claude/skills/)
    pub setup_claude: bool,
    /// Whether to set up AGENTS.md (Codex CLI)
    pub setup_agents: bool,
    /// Which checkbox is currently focused
    pub focus: InitFocus,
}

impl Default for InitState {
    fn default() -> Self {
        Self {
            setup_claude: true,
            setup_agents: true,
            focus: InitFocus::Claude,
        }
    }
}

/// Action to confirm.
#[derive(Debug)]
pub enum ConfirmAction {
    InitRepo {
        setup_claude: bool,
        setup_agents: bool,
    },
    Close(String),
    Reopen(String),
    BulkClose(Vec<String>),
    BulkReopen(Vec<String>),
    BulkSetPriority { ids: Vec<String>, priority: Priority },
    BulkSetSeverity { ids: Vec<String>, severity: Severity },
    BulkSetStatus { ids: Vec<String>, status: TicketStatus },
    BulkSetAssignees { ids: Vec<String>, assignees: Vec<String> },
    BulkSetMilestone {
        ids: Vec<String>,
        milestone_id: Option<String>,
    },
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
    EditTicketEditor(String),
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
    BulkPriority(Vec<String>),
    BulkSeverity(Vec<String>),
    BulkStatus(Vec<String>),
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
    pub fn priority(ticket_id: String, allowed: Vec<String>) -> Self {
        let values = resolve_allowed_list(allowed, &DEFAULT_PRIORITIES);
        Self {
            title: "Select Priority".to_string(),
            kind: SelectKind::Priority(ticket_id),
            options: select_options_from_values(&values),
            selected: 0,
        }
    }

    pub fn priority_bulk(ids: Vec<String>, allowed: Vec<String>) -> Self {
        let values = resolve_allowed_list(allowed, &DEFAULT_PRIORITIES);
        Self {
            title: "Select Priority (Bulk)".to_string(),
            kind: SelectKind::BulkPriority(ids),
            options: select_options_from_values(&values),
            selected: 0,
        }
    }

    pub fn severity(ticket_id: String, allowed: Vec<String>) -> Self {
        let values = resolve_allowed_list(allowed, &DEFAULT_SEVERITIES);
        Self {
            title: "Select Severity".to_string(),
            kind: SelectKind::Severity(ticket_id),
            options: select_options_from_values(&values),
            selected: 0,
        }
    }

    pub fn severity_bulk(ids: Vec<String>, allowed: Vec<String>) -> Self {
        let values = resolve_allowed_list(allowed, &DEFAULT_SEVERITIES);
        Self {
            title: "Select Severity (Bulk)".to_string(),
            kind: SelectKind::BulkSeverity(ids),
            options: select_options_from_values(&values),
            selected: 0,
        }
    }

    pub fn status(ticket_id: String, allowed: Vec<String>) -> Self {
        let values = resolve_allowed_list(allowed, &DEFAULT_STATUSES);
        Self {
            title: "Select Status".to_string(),
            kind: SelectKind::Status(ticket_id),
            options: select_options_from_values(&values),
            selected: 0,
        }
    }

    pub fn status_bulk(ids: Vec<String>, allowed: Vec<String>) -> Self {
        let values = resolve_allowed_list(allowed, &DEFAULT_STATUSES);
        Self {
            title: "Select Status (Bulk)".to_string(),
            kind: SelectKind::BulkStatus(ids),
            options: select_options_from_values(&values),
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

const DEFAULT_PRIORITIES: [&str; 4] = ["low", "medium", "high", "critical"];
const DEFAULT_SEVERITIES: [&str; 4] = ["low", "normal", "high", "critical"];
const DEFAULT_STATUSES: [&str; 5] = ["open", "in_progress", "blocked", "closed", "archived"];

fn resolve_allowed_list(allowed: Vec<String>, fallback: &[&str]) -> Vec<String> {
    if allowed.is_empty() {
        fallback.iter().map(|value| (*value).to_string()).collect()
    } else {
        allowed
    }
}

fn select_options_from_values(values: &[String]) -> Vec<SelectOption> {
    let mut used_hotkeys = HashSet::new();
    values
        .iter()
        .map(|value| {
            let hotkey = value
                .chars()
                .find(|ch| ch.is_ascii_alphanumeric())
                .map(|ch| ch.to_ascii_lowercase())
                .filter(|ch| used_hotkeys.insert(*ch));
            SelectOption {
                label: titleize(value),
                value: value.clone(),
                hotkey,
            }
        })
        .collect()
}

fn titleize(value: &str) -> String {
    value
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let rest = chars.as_str().to_ascii_lowercase();
                    format!("{}{}", first.to_ascii_uppercase(), rest)
                }
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// State for milestone menu.
#[derive(Debug)]
pub struct MilestoneMenuState {
    pub ticket_ids: Vec<String>,
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

/// Step in the relation edit workflow.
#[derive(Debug, Clone)]
pub enum RelationStep {
    /// Select relation type.
    SelectType,
    /// Search for target ticket.
    SearchTarget,
}

/// State for relation edit mode.
#[derive(Debug)]
pub struct RelationEditState {
    pub ticket_id: String,
    pub step: RelationStep,
    /// Selected relation type (if chosen).
    pub relation_type: Option<RelationType>,
    /// All available relation types.
    pub relation_types: Vec<RelationType>,
    /// Selected index for type selection.
    pub type_selected: usize,
    /// Search query for target ticket.
    pub search_query: String,
    /// Cursor position in search.
    pub search_cursor: usize,
    /// Filtered tickets matching search.
    pub filtered_tickets: Vec<TicketSummary>,
    /// Selected ticket in filtered list.
    pub ticket_selected: usize,
    /// All tickets (for filtering).
    pub all_tickets: Vec<TicketSummary>,
}

/// Minimal ticket info for relation selection.
#[derive(Debug, Clone)]
pub struct TicketSummary {
    pub id: String,
    pub title: String,
    pub status: TicketStatus,
}

impl RelationEditState {
    pub fn new(ticket_id: String, all_tickets: Vec<TicketSummary>) -> Self {
        Self {
            ticket_id,
            step: RelationStep::SelectType,
            relation_type: None,
            relation_types: vec![
                RelationType::Blocks,
                RelationType::BlockedBy,
                RelationType::DependsOn,
                RelationType::Duplicate,
                RelationType::Parent,
                RelationType::Child,
            ],
            type_selected: 0,
            search_query: String::new(),
            search_cursor: 0,
            filtered_tickets: all_tickets.clone(),
            ticket_selected: 0,
            all_tickets,
        }
    }

    /// Update filtered tickets based on search query.
    pub fn update_filter(&mut self) {
        let query = self.search_query.to_lowercase();
        if query.is_empty() {
            self.filtered_tickets = self
                .all_tickets
                .iter()
                .filter(|t| t.id != self.ticket_id)
                .cloned()
                .collect();
        } else {
            self.filtered_tickets = self
                .all_tickets
                .iter()
                .filter(|t| {
                    t.id != self.ticket_id
                        && (t.id.to_lowercase().contains(&query)
                            || t.title.to_lowercase().contains(&query))
                })
                .cloned()
                .collect();
        }
        if self.ticket_selected >= self.filtered_tickets.len() {
            self.ticket_selected = self.filtered_tickets.len().saturating_sub(1);
        }
    }

    /// Move to next relation type.
    pub fn next_type(&mut self) {
        if self.type_selected + 1 < self.relation_types.len() {
            self.type_selected += 1;
        }
    }

    /// Move to previous relation type.
    pub fn prev_type(&mut self) {
        if self.type_selected > 0 {
            self.type_selected -= 1;
        }
    }

    /// Move to next ticket.
    pub fn next_ticket(&mut self) {
        if self.ticket_selected + 1 < self.filtered_tickets.len() {
            self.ticket_selected += 1;
        }
    }

    /// Move to previous ticket.
    pub fn prev_ticket(&mut self) {
        if self.ticket_selected > 0 {
            self.ticket_selected -= 1;
        }
    }

    /// Select current type and move to search step.
    pub fn select_type(&mut self) {
        if let Some(rt) = self.relation_types.get(self.type_selected) {
            self.relation_type = Some(rt.clone());
            self.step = RelationStep::SearchTarget;
            self.update_filter();
        }
    }

    /// Get the selected target ticket.
    pub fn selected_target(&self) -> Option<&TicketSummary> {
        self.filtered_tickets.get(self.ticket_selected)
    }
}

/// Cached details for the selected ticket.
#[derive(Debug, Default)]
pub struct TicketDetails {
    pub ticket_id: Option<String>,
    pub notes: Option<String>,
    pub events: Vec<Event>,
    pub scroll_offset: usize,
    /// Selected relation index (for Relations tab).
    pub relation_selected: usize,
    pub relation_targets: Vec<Option<TicketSummary>>,
    pub milestone: Option<Milestone>,
    /// Diff state for version comparison.
    pub diff_state: DiffState,
}

impl TicketDetails {
    pub fn clear(&mut self) {
        self.ticket_id = None;
        self.notes = None;
        self.events.clear();
        self.scroll_offset = 0;
        self.relation_selected = 0;
        self.relation_targets.clear();
        self.milestone = None;
        self.diff_state.clear();
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
        self.relation_selected = 0;
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
    use tik_core::GraphEdge;

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
        assert_eq!(tab.prev(), DetailsTab::Diff);
    }

    #[test]
    fn tag_edit_state_suggests_and_adds_tags() {
        let mut state = TagEditState::new(
            "T-1".to_string(),
            vec!["core".to_string()],
            vec!["core".to_string(), "cli".to_string(), "ui".to_string()],
        );
        state.input = "c".to_string();
        state.update_suggestions();
        assert_eq!(state.suggestions, vec!["cli".to_string()]);
        assert_eq!(state.suggestion_idx, Some(0));
        assert!(state.add_selected_suggestion());
        assert!(state.tags.contains(&"cli".to_string()));
        state.input = "New Tag".to_string();
        assert!(state.add_current_tag());
        assert!(state.tags.contains(&"new-tag".to_string()));
        state.input.clear();
        assert!(state.remove_last_tag());
    }

    #[test]
    fn relation_edit_filters_and_selects() {
        let all = vec![
            TicketSummary {
                id: "T-1".to_string(),
                title: "Alpha".to_string(),
                status: TicketStatus::Open,
            },
            TicketSummary {
                id: "T-2".to_string(),
                title: "Beta".to_string(),
                status: TicketStatus::Closed,
            },
        ];
        let mut state = RelationEditState::new("T-1".to_string(), all);
        state.update_filter();
        assert_eq!(state.filtered_tickets.len(), 1);
        state.search_query = "beta".to_string();
        state.update_filter();
        assert_eq!(state.filtered_tickets[0].id, "T-2");
        state.next_type();
        state.prev_type();
        state.select_type();
        assert!(matches!(state.step, RelationStep::SearchTarget));
        assert!(state.selected_target().is_some());
    }

    #[test]
    fn diff_state_navigation_bounds() {
        let snapshot = DiffSnapshot {
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            actor: "human".to_string(),
            content: "{}".to_string(),
        };
        let mut state = DiffState {
            snapshots: vec![snapshot.clone(), snapshot.clone(), snapshot],
            before_idx: 0,
            after_idx: 1,
            diff_lines: Vec::new(),
        };
        state.next_after();
        assert_eq!(state.after_idx, 2);
        state.prev_after();
        assert_eq!(state.after_idx, 1);
    }

    #[test]
    fn graph_state_renders_empty_and_focuses_nodes() {
        let graph = Graph {
            nodes: Vec::new(),
            edges: Vec::new(),
        };
        let empty = GraphState::new(graph, None);
        assert_eq!(empty.rendered_lines.len(), 1);
        assert!(empty.rendered_lines[0]
            .content
            .contains("No tickets with relations"));
        assert!(empty.focused_node_id().is_none());

        let graph = Graph {
            nodes: vec![
                GraphNode {
                    id: "T-1".to_string(),
                    title: "Alpha".to_string(),
                    status: "open".to_string(),
                },
                GraphNode {
                    id: "T-2".to_string(),
                    title: "Beta".to_string(),
                    status: "closed".to_string(),
                },
            ],
            edges: vec![GraphEdge {
                from: "T-1".to_string(),
                to: "T-2".to_string(),
                relation: "blocks".to_string(),
            }],
        };
        let state = GraphState::new(graph, None);
        assert_eq!(state.focused_node_id(), Some("T-1"));
    }

    #[test]
    fn ticket_details_clear_resets_state() {
        let mut details = TicketDetails {
            ticket_id: Some("T-1".to_string()),
            notes: Some("notes".to_string()),
            events: vec![Event::note("actor", "ts", "text")],
            scroll_offset: 2,
            relation_selected: 3,
            relation_targets: Vec::new(),
            milestone: None,
            diff_state: DiffState {
                snapshots: vec![DiffSnapshot {
                    timestamp: "ts".to_string(),
                    actor: "actor".to_string(),
                    content: "{}".to_string(),
                }],
                before_idx: 1,
                after_idx: 1,
                diff_lines: vec![DiffLine::Added("x".to_string())],
            },
        };
        details.clear();
        assert!(details.ticket_id.is_none());
        assert!(details.notes.is_none());
        assert!(details.events.is_empty());
        assert_eq!(details.scroll_offset, 0);
        assert_eq!(details.relation_selected, 0);
        assert!(details.relation_targets.is_empty());
        assert!(details.milestone.is_none());
        assert!(details.diff_state.snapshots.is_empty());
        assert!(details.diff_state.diff_lines.is_empty());
    }

    #[test]
    fn short_id_returns_input() {
        assert_eq!(short_id("T-123"), "T-123");
        assert_eq!(short_id("T-01KFGFZN1W0WM6T4586T8FPV46"), "T-01KFGFZN1W0WM6T4586T8FPV46");
    }
}
