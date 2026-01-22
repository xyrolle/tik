//! TUI rendering functions.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap};
use ratatui::Frame;
use tik_core::Ticket;

use super::app::App;
use super::state::{
    ActionState, ConfirmState, DetailsTab, DiffLine, GraphState, InitFocus, InputState,
    MilestoneMenuState, Panel, RelationEditState, RelationStep, SearchState, SelectState,
    TagEditState,
};
use super::style::{
    active_tab_style, badge_style, disabled_style, event_icon, event_type_style,
    focused_border_style, highlight_style, inactive_tab_style, key_hint_style, priority_style,
    priority_symbol, status_style, status_symbol, unfocused_border_style,
};

/// Main draw entry point.
pub fn draw(frame: &mut Frame, app: &App) {
    if app.repo.is_none() {
        draw_welcome(frame, app);
        // Still draw overlays even when repo is not initialized
        draw_overlays(frame, app);
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(4), // Increased for two-line footer
        ])
        .split(frame.size());

    draw_header(frame, layout[0], app);
    draw_body(frame, layout[1], app);
    draw_footer(frame, layout[2], app);

    draw_overlays(frame, app);
}

/// Draw modal overlays based on current mode.
fn draw_overlays(frame: &mut Frame, app: &App) {
    match &app.mode {
        super::state::Mode::Input(state) => draw_input_overlay(frame, state),
        super::state::Mode::Confirm(state) => draw_confirm_overlay(frame, state),
        super::state::Mode::Init(state) => draw_init_overlay(frame, state, app),
        super::state::Mode::Action(state) => draw_action_overlay(frame, state, app),
        super::state::Mode::Search(state) => draw_search_overlay(frame, state, app),
        super::state::Mode::Select(state) => draw_select_overlay(frame, state, app),
        super::state::Mode::Help => draw_help_overlay(frame, app),
        super::state::Mode::MilestoneMenu(state) => draw_milestone_overlay(frame, state, app),
        super::state::Mode::TagEdit(state) => draw_tag_edit_overlay(frame, state, app),
        super::state::Mode::RelationEdit(state) => draw_relation_edit_overlay(frame, state, app),
        super::state::Mode::GraphView(state) => draw_graph_overlay(frame, state, app),
        super::state::Mode::Normal => {}
    }
}

/// Draw welcome screen for uninitialized repo.
fn draw_welcome(frame: &mut Frame, app: &App) {
    let area = frame.size();
    let block = Block::default().borders(Borders::ALL).title("Tiketer");
    frame.render_widget(block, area);
    let message = vec![
        Line::from("No .tik repo found in this directory."),
        Line::from(""),
        Line::from("Press i to initialize here, or q to quit."),
        Line::from(format!("Root: {}", app.root.display())),
    ];
    let paragraph = Paragraph::new(message)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, centered_rect(80, 40, area));
}

/// Draw header with repo info and filter badges.
fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![
        Span::styled("Tiketer", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
    ];

    // Filter badge (only if not "all")
    if app.filter != super::state::TicketFilter::All {
        spans.push(Span::styled(
            format!(" {} ", app.filter.label()),
            badge_style(app.no_color),
        ));
        spans.push(Span::raw(" "));
    }

    // Search badge
    if !app.search_query.is_empty() {
        spans.push(Span::styled(
            format!(" search:{} ", app.search_query),
            badge_style(app.no_color),
        ));
        spans.push(Span::raw(" "));
    }

    // Ticket count
    let count_style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(Color::DarkGray)
    };
    spans.push(Span::styled(
        format!("{} tickets", app.tickets.len()),
        count_style,
    ));

    let title = Line::from(spans);
    let paragraph = Paragraph::new(title).block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

/// Draw main body with ticket list and details.
fn draw_body(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    draw_ticket_list(frame, chunks[0], app);
    draw_details_panel(frame, chunks[1], app);
}

/// Draw ticket list panel.
fn draw_ticket_list(frame: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focused_panel == Panel::TicketList;
    let border_style = if is_focused {
        focused_border_style(app.no_color)
    } else {
        unfocused_border_style(app.no_color)
    };

    // Focus indicator: ◆ for focused, ○ for unfocused
    let focus_indicator = if is_focused { "◆" } else { "○" };

    // Calculate navigation indicators (show if there are items above/below selection)
    let total_items = app.tickets.len();
    let can_scroll_up = app.selected > 0;
    let can_scroll_down = app.selected + 1 < total_items;

    let scroll_indicator = match (can_scroll_up, can_scroll_down) {
        (true, true) => " ▲▼",
        (true, false) => " ▲",
        (false, true) => " ▼",
        (false, false) => "",
    };

    let list_title = if app.tickets.is_empty() {
        format!("{} [1] Tickets 0/0", focus_indicator)
    } else {
        format!(
            "{} [1] Tickets {}/{}{}",
            focus_indicator,
            app.selected + 1,
            app.tickets.len(),
            scroll_indicator
        )
    };

    let items: Vec<ListItem> = app
        .tickets
        .iter()
        .map(|ticket| {
            let is_selected = app.is_selected(ticket.id.as_str());
            ticket_list_item(ticket, app.no_color, is_selected)
        })
        .collect();

    let mut state = ListState::default();
    if !app.tickets.is_empty() {
        state.select(Some(app.selected));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(list_title),
        )
        .highlight_symbol(">> ")
        .highlight_style(highlight_style(app.no_color));
    frame.render_stateful_widget(list, area, &mut state);
}

/// Draw details panel with tabs.
fn draw_details_panel(frame: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focused_panel == Panel::Details;
    let border_style = if is_focused {
        focused_border_style(app.no_color)
    } else {
        unfocused_border_style(app.no_color)
    };

    // Focus indicator: ◆ for focused, ○ for unfocused
    let focus_indicator = if is_focused { "◆" } else { "○" };

    // Scroll indicator for details content
    let scroll_offset = app.ticket_details.scroll_offset;
    let can_scroll_up = scroll_offset > 0;
    // We don't know exact content height without rendering, but show indicator if scrolled
    let scroll_indicator = if can_scroll_up { " ▲" } else { "" };

    let details_title = app
        .tickets
        .get(app.selected)
        .map(|ticket| {
            format!(
                "{} [2] {}{}",
                focus_indicator,
                ticket.id.as_str(),
                scroll_indicator
            )
        })
        .unwrap_or_else(|| format!("{} [2] Details", focus_indicator));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(details_title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.tickets.is_empty() {
        let paragraph = Paragraph::new("No ticket selected.").wrap(Wrap { trim: true });
        frame.render_widget(paragraph, inner);
        return;
    }

    // Tab bar
    let tab_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    let content_area = Rect {
        x: inner.x,
        y: inner.y + 1,
        width: inner.width,
        height: inner.height.saturating_sub(1),
    };

    draw_tab_bar(frame, tab_area, app);

    // Tab content
    match app.details_tab {
        DetailsTab::Info => draw_info_tab(frame, content_area, app),
        DetailsTab::Notes => draw_notes_tab(frame, content_area, app),
        DetailsTab::History => draw_history_tab(frame, content_area, app),
        DetailsTab::Relations => draw_relations_tab(frame, content_area, app),
        DetailsTab::Diff => draw_diff_tab(frame, content_area, app),
    }
}

/// Draw tab bar in details panel.
fn draw_tab_bar(frame: &mut Frame, area: Rect, app: &App) {
    let is_focused = app.focused_panel == Panel::Details;
    let hint_style = if app.no_color {
        Style::default().add_modifier(Modifier::DIM)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    // Build tab titles with number shortcuts
    let titles: Vec<Line> = [
        DetailsTab::Info,
        DetailsTab::Notes,
        DetailsTab::History,
        DetailsTab::Relations,
        DetailsTab::Diff,
    ]
    .iter()
    .enumerate()
    .map(|(i, tab)| {
        let style = if i == app.details_tab.index() {
            active_tab_style(app.no_color)
        } else {
            inactive_tab_style(app.no_color)
        };
        let label = format!(" {} ", tab.label());
        Line::from(Span::styled(label, style))
    })
    .collect();

    // Show navigation hint when details panel is focused
    let divider = if is_focused { "│" } else { "|" };

    let tabs = Tabs::new(titles)
        .select(app.details_tab.index())
        .divider(divider);

    // Render tabs
    frame.render_widget(tabs, area);

    // Add navigation hint at the end when focused
    if is_focused && area.width > 40 {
        let hint = Span::styled(" ←/→", hint_style);
        let hint_area = Rect {
            x: area.x + area.width.saturating_sub(5),
            y: area.y,
            width: 5,
            height: 1,
        };
        frame.render_widget(Paragraph::new(hint), hint_area);
    }
}

/// Draw Info tab content.
fn draw_info_tab(frame: &mut Frame, area: Rect, app: &App) {
    let Some(ticket) = app.tickets.get(app.selected) else {
        return;
    };

    let status_style_val = if app.no_color {
        status_style(&ticket.status, true)
    } else {
        status_style(&ticket.status, false).add_modifier(Modifier::BOLD)
    };

    let summary = if ticket.summary.trim().is_empty() {
        "-".to_string()
    } else {
        ticket.summary.clone()
    };

    let description = if ticket.description.trim().is_empty() {
        "-".to_string()
    } else {
        ticket.description.clone()
    };

    let mut lines = vec![
        Line::from(vec![Span::raw("ID: "), Span::raw(ticket.id.as_str())]),
        Line::from(vec![Span::raw("Title: "), Span::raw(ticket.title.as_str())]),
        Line::from(vec![
            Span::raw("Status: "),
            Span::styled(
                format!(
                    "{} {}",
                    status_symbol(&ticket.status),
                    ticket.status.as_str()
                ),
                status_style_val,
            ),
        ]),
        Line::from(format!("Type: {}", ticket.kind.as_str())),
        Line::from(vec![
            Span::raw("Priority: "),
            Span::styled(
                format!(
                    "{}{}",
                    priority_symbol(&ticket.priority),
                    ticket.priority.as_str()
                ),
                priority_style(&ticket.priority, app.no_color),
            ),
        ]),
        Line::from(format!("Severity: {}", ticket.severity.as_str())),
        Line::from(format!("Assignees: {}", join_or_dash(&ticket.assignees))),
        Line::from(format!("Tags: {}", join_or_dash(&ticket.tags))),
        Line::from(format!("Created: {}", ticket.created_at)),
        Line::from(format!("Updated: {}", ticket.updated_at)),
        Line::from(format!(
            "Closed: {}",
            ticket.closed_at.as_deref().unwrap_or("-")
        )),
    ];

    if let Some(due) = &ticket.due_at {
        lines.push(Line::from(format!("Due: {}", due)));
    }

    if let Some(ref milestone_id) = ticket.milestone_id {
        lines.push(Line::from(format!("Milestone: {}", milestone_id.as_str())));
    }

    lines.push(Line::from(""));
    lines.push(Line::from("Summary:"));
    lines.push(Line::from(summary));
    lines.push(Line::from(""));
    lines.push(Line::from("Description:"));
    for line in description.lines() {
        lines.push(Line::from(line.to_string()));
    }

    if !ticket.acceptance.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from("Acceptance Criteria:"));
        for (i, criteria) in ticket.acceptance.iter().enumerate() {
            lines.push(Line::from(format!("  {}. {}", i + 1, criteria)));
        }
    }

    let scroll = app.ticket_details.scroll_offset;
    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).collect();

    let paragraph = Paragraph::new(visible_lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

/// Draw Notes tab content.
fn draw_notes_tab(frame: &mut Frame, area: Rect, app: &App) {
    let content = app
        .ticket_details
        .notes
        .as_deref()
        .unwrap_or("Loading notes...");

    let lines: Vec<Line> = content.lines().map(|l| Line::from(l.to_string())).collect();

    let scroll = app.ticket_details.scroll_offset;
    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).collect();

    let paragraph = Paragraph::new(visible_lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

/// Draw History tab content.
fn draw_history_tab(frame: &mut Frame, area: Rect, app: &App) {
    if app.ticket_details.events.is_empty() {
        let paragraph = Paragraph::new("No events recorded.").wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
        return;
    }

    let lines: Vec<Line> = app
        .ticket_details
        .events
        .iter()
        .map(|event| {
            let icon = event_icon(&event.kind);
            let style = event_type_style(&event.kind, app.no_color);
            Line::from(vec![
                Span::styled(format!("{} ", icon), style),
                Span::styled(format!("{:<15}", event.kind), style),
                Span::raw(format!(" {} ", event.actor)),
                Span::styled(
                    event.ts.clone(),
                    Style::default().add_modifier(Modifier::DIM),
                ),
            ])
        })
        .collect();

    let scroll = app.ticket_details.scroll_offset;
    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).collect();

    let paragraph = Paragraph::new(visible_lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

/// Draw Relations tab content.
fn draw_relations_tab(frame: &mut Frame, area: Rect, app: &App) {
    let Some(ticket) = app.tickets.get(app.selected) else {
        return;
    };

    let is_focused = app.focused_panel == Panel::Details;
    let highlight_style = highlight_style(app.no_color);

    let mut lines = Vec::new();

    // Relations header with hint
    if is_focused && !ticket.relations.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("Relations: "),
            Span::styled(
                "(+add d:del Enter:jump)",
                Style::default().add_modifier(Modifier::DIM),
            ),
        ]));
    } else {
        lines.push(Line::from("Relations:"));
    }

    if ticket.relations.is_empty() {
        if is_focused {
            lines.push(Line::from(vec![
                Span::raw("  (none) "),
                Span::styled("+ to add", Style::default().add_modifier(Modifier::DIM)),
            ]));
        } else {
            lines.push(Line::from("  (none)"));
        }
    } else {
        for (i, rel) in ticket.relations.iter().enumerate() {
            let is_selected = is_focused && i == app.ticket_details.relation_selected;
            let marker = if is_selected { ">> " } else { "   " };
            let line_style = if is_selected {
                highlight_style
            } else {
                Style::default()
            };
            let target_label = match app
                .ticket_details
                .relation_targets
                .get(i)
                .and_then(|target| target.as_ref())
            {
                Some(target) => format!(
                    "{} [{}] {}",
                    target.id,
                    target.status.as_str(),
                    target.title
                ),
                None => format!("{} (missing)", rel.id.as_str()),
            };
            lines.push(Line::from(vec![
                Span::styled(marker, line_style),
                Span::styled(
                    format!("{:<12} {}", rel.kind.as_str(), target_label),
                    line_style,
                ),
            ]));
        }
    }

    lines.push(Line::from(""));

    // Artifacts
    lines.push(Line::from("Artifacts:"));
    if ticket.artifacts.is_empty() {
        lines.push(Line::from("  (none)"));
    } else {
        for artifact in &ticket.artifacts {
            lines.push(Line::from(format!(
                "   [{}] {}",
                artifact.kind.as_str(),
                artifact.reference
            )));
        }
    }

    lines.push(Line::from(""));

    // Milestone
    lines.push(Line::from("Milestone:"));
    if let Some(ref milestone_id) = ticket.milestone_id {
        if let Some(milestone) = app.ticket_details.milestone.as_ref() {
            lines.push(Line::from(format!(
                "   {} [{}] {}",
                milestone.id,
                milestone.status.as_str(),
                milestone.title
            )));
        } else {
            lines.push(Line::from(format!(
                "   {} (missing)",
                milestone_id.as_str()
            )));
        }
    } else {
        lines.push(Line::from("   (none)"));
    }

    let scroll = app.ticket_details.scroll_offset;
    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).collect();

    let paragraph = Paragraph::new(visible_lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

/// Draw Diff tab content.
fn draw_diff_tab(frame: &mut Frame, area: Rect, app: &App) {
    let diff_state = &app.ticket_details.diff_state;

    let hint_style = if app.no_color {
        Style::default().add_modifier(Modifier::DIM)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let added_style = if app.no_color {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };

    let removed_style = if app.no_color {
        Style::default().add_modifier(Modifier::DIM)
    } else {
        Style::default().fg(Color::Red)
    };

    let header_style = if app.no_color {
        Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    };

    let mut lines: Vec<Line> = Vec::new();

    if diff_state.snapshots.len() < 2 {
        lines.push(Line::from("Not enough versions to compare."));
        lines.push(Line::from(Span::styled(
            "Need at least 2 ticket versions (created + edit events).",
            hint_style,
        )));
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
        return;
    }

    // Version selector header
    let before = &diff_state.snapshots[diff_state.before_idx];
    let after = &diff_state.snapshots[diff_state.after_idx];

    lines.push(Line::from(vec![
        Span::styled("Comparing: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("{} ", before.timestamp), removed_style),
        Span::raw("→ "),
        Span::styled(format!("{}", after.timestamp), added_style),
    ]));
    lines.push(Line::from(Span::styled(
        format!(
            "({}/{} versions) • j/k: scroll • </> change versions",
            diff_state.after_idx + 1,
            diff_state.snapshots.len()
        ),
        hint_style,
    )));
    lines.push(Line::from(""));

    // Diff content
    for diff_line in &diff_state.diff_lines {
        match diff_line {
            DiffLine::Context(text) => {
                lines.push(Line::from(format!("  {}", text)));
            }
            DiffLine::Added(text) => {
                lines.push(Line::from(Span::styled(format!("+ {}", text), added_style)));
            }
            DiffLine::Removed(text) => {
                lines.push(Line::from(Span::styled(format!("- {}", text), removed_style)));
            }
            DiffLine::Header(text) => {
                lines.push(Line::from(Span::styled(text.clone(), header_style)));
            }
        }
    }

    let scroll = app.ticket_details.scroll_offset;
    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).collect();

    let paragraph = Paragraph::new(visible_lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

/// Draw context-sensitive footer with two lines.
fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Line 1: Mode indicator │ Position │ Breadcrumb │ Status
    let line1 = build_status_line(app);

    // Line 2: Key hints with dividers
    let line2 = build_hints_line(app);

    let content = vec![line1, line2];
    let paragraph = Paragraph::new(content);
    frame.render_widget(paragraph, inner);
}

/// Build the status line (first line of footer).
fn build_status_line(app: &App) -> Line<'static> {
    let mode_style = if app.no_color {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    };
    let dim_style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let status_style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(Color::Yellow)
    };

    // Mode indicator
    let mode_name = if app.selection_mode {
        "SELECTION"
    } else {
        match &app.mode {
            super::state::Mode::Normal => "NORMAL",
            super::state::Mode::Input(_) => "INPUT",
            super::state::Mode::Confirm(_) => "CONFIRM",
            super::state::Mode::Init(_) => "INIT",
            super::state::Mode::Action(_) => "ACTION",
            super::state::Mode::Search(_) => "SEARCH",
            super::state::Mode::Select(_) => "SELECT",
            super::state::Mode::Help => "HELP",
            super::state::Mode::MilestoneMenu(_) => "MILESTONE",
            super::state::Mode::TagEdit(_) => "TAGS",
            super::state::Mode::RelationEdit(_) => "RELATION",
            super::state::Mode::GraphView(_) => "GRAPH",
        }
    };

    // Position indicator
    let position = if app.tickets.is_empty() {
        "0/0".to_string()
    } else {
        format!("{}/{}", app.selected + 1, app.tickets.len())
    };

    // Breadcrumb: Filter › Panel › Tab
    let filter_name = app.filter.label();
    let panel_name = match app.focused_panel {
        Panel::TicketList => "Tickets",
        Panel::Details => "Details",
    };
    let tab_name = match app.details_tab {
        super::state::DetailsTab::Info => "Info",
        super::state::DetailsTab::Notes => "Notes",
        super::state::DetailsTab::History => "History",
        super::state::DetailsTab::Relations => "Relations",
        super::state::DetailsTab::Diff => "Diff",
    };

    let breadcrumb = if app.focused_panel == Panel::Details {
        format!("{} › {} › {}", filter_name, panel_name, tab_name)
    } else {
        format!("{} › {}", filter_name, panel_name)
    };

    // Status message (truncated if needed)
    let status_msg = if app.status.is_empty() {
        String::new()
    } else {
        format!("│ {}", app.status)
    };

    Line::from(vec![
        Span::styled(format!("-- {} --", mode_name), mode_style),
        Span::styled("  ", dim_style),
        Span::styled(format!("[{}]", position), dim_style),
        Span::styled("  ", dim_style),
        Span::raw(breadcrumb),
        Span::styled(status_msg, status_style),
    ])
}

/// Build the hints line (second line of footer).
fn build_hints_line(app: &App) -> Line<'static> {
    let key_style = key_hint_style(app.no_color);
    let dim_style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let mut spans: Vec<Span> = Vec::new();

    // Helper to add a hint group
    let mut add_hint = |key: &'static str, desc: &'static str, first: bool| {
        if !first {
            spans.push(Span::styled(" │ ", dim_style));
        }
        spans.push(Span::styled(key, key_style));
        spans.push(Span::raw(format!(" {}", desc)));
    };

    // Common hints
    add_hint("q", "quit", true);
    add_hint("?", "help", false);

    match app.focused_panel {
        Panel::TicketList => {
            add_hint("j/k", "move", false);
            add_hint("Tab", "pane", false);
            add_hint("/", "search", false);
            add_hint("n", "new", false);
            if !app.tickets.is_empty() {
                add_hint("e", "edit", false);
                add_hint("o", "editor", false);
                add_hint("p", "priority", false);
                add_hint("s", "status", false);
            }
        }
        Panel::Details => {
            add_hint("j/k", "scroll", false);
            add_hint("Tab", "pane", false);
            add_hint("←/→", "tabs", false);
        }
    }

    if !app.search_query.is_empty() {
        add_hint("x", "clear", false);
    }

    Line::from(spans)
}

/// Draw input overlay.
pub fn draw_input_overlay(frame: &mut Frame, state: &InputState) {
    let area = centered_rect(80, 50, frame.size());
    frame.render_widget(Clear, area);
    let block = Block::default().borders(Borders::ALL).title(state.title);
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(3), // Leave room for hints
    };

    // Build field lines
    let lines: Vec<Line> = state
        .fields
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let marker = if idx == state.current { ">" } else { " " };
            let value = if field.value.is_empty() && !field.required {
                String::new()
            } else {
                field.value.clone()
            };
            Line::from(format!("{marker} {}: {}", field.label, value))
        })
        .collect();
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);

    // Draw key hints at bottom
    let hints_area = Rect {
        x: area.x + 2,
        y: area.y + area.height.saturating_sub(2),
        width: area.width.saturating_sub(4),
        height: 1,
    };
    let hints = Line::from(vec![
        Span::styled("Ctrl+S", Style::default().fg(Color::Cyan)),
        Span::raw(" Save  "),
        Span::styled("Tab/↓", Style::default().fg(Color::Cyan)),
        Span::raw(" Next  "),
        Span::styled("Shift+Tab/↑", Style::default().fg(Color::Cyan)),
        Span::raw(" Prev  "),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::raw(" Cancel"),
    ]);
    frame.render_widget(Paragraph::new(hints), hints_area);

    if let Some(field) = state.fields.get(state.current) {
        let label_len = field.label.len();
        let cursor_x = inner.x + 4 + label_len as u16 + state.cursor as u16;
        let cursor_y = inner.y + state.current as u16;
        let max_x = inner.x + inner.width.saturating_sub(1);
        frame.set_cursor(cursor_x.min(max_x), cursor_y);
    }
}

/// Draw confirmation overlay.
pub fn draw_confirm_overlay(frame: &mut Frame, state: &ConfirmState) {
    let area = centered_rect(60, 30, frame.size());
    frame.render_widget(Clear, area);
    let block = Block::default().borders(Borders::ALL).title("Confirm");
    frame.render_widget(block, area);
    let text = vec![
        Line::from(state.prompt.clone()),
        Line::from(""),
        Line::from("Press y to confirm, n to cancel."),
    ];
    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

/// Draw init overlay with AI setup checkboxes.
fn draw_init_overlay(frame: &mut Frame, state: &super::state::InitState, app: &App) {
    let area = centered_rect(55, 35, frame.size());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Initialize Repository ")
        .title_alignment(Alignment::Center);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Layout: header, checkboxes, footer
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Root path
            Constraint::Length(1), // Spacing
            Constraint::Length(4), // Checkboxes
            Constraint::Min(1),    // Spacing
            Constraint::Length(1), // Key hints
        ])
        .split(inner);

    // Root path (left-aligned with padding)
    let root_text = format!("  Root: {}", app.root.display());
    let root_para = Paragraph::new(root_text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(root_para, layout[0]);

    // Checkboxes
    let checkbox = |checked: bool| if checked { "[x]" } else { "[ ]" };
    let focused_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let normal_style = Style::default();
    let dim_style = Style::default().fg(Color::DarkGray);

    let claude_focused = state.focus == InitFocus::Claude;
    let agents_focused = state.focus == InitFocus::Agents;

    let checkboxes = vec![
        // Claude Code
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                if claude_focused { "▸ " } else { "  " },
                if claude_focused { focused_style } else { normal_style },
            ),
            Span::styled(
                checkbox(state.setup_claude),
                if claude_focused { focused_style } else { normal_style },
            ),
            Span::styled(
                " Claude Code",
                if claude_focused { focused_style } else { normal_style },
            ),
            Span::styled("  CLAUDE.md, .claude/skills/", dim_style),
        ]),
        // Codex CLI
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                if agents_focused { "▸ " } else { "  " },
                if agents_focused { focused_style } else { normal_style },
            ),
            Span::styled(
                checkbox(state.setup_agents),
                if agents_focused { focused_style } else { normal_style },
            ),
            Span::styled(
                " Codex CLI",
                if agents_focused { focused_style } else { normal_style },
            ),
            Span::styled("    AGENTS.md", dim_style),
        ]),
    ];
    let checkbox_para = Paragraph::new(checkboxes);
    frame.render_widget(checkbox_para, layout[2]);

    // Key hints (centered)
    let hints = Line::from(vec![
        Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" move  ", dim_style),
        Span::styled("Space", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" toggle  ", dim_style),
        Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" confirm  ", dim_style),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" cancel", dim_style),
    ]);
    let hints_para = Paragraph::new(hints).alignment(Alignment::Center);
    frame.render_widget(hints_para, layout[4]);
}

/// Draw action menu overlay.
pub fn draw_action_overlay(frame: &mut Frame, state: &ActionState, app: &App) {
    let area = centered_rect(60, 60, frame.size());
    frame.render_widget(Clear, area);
    let title = app
        .tickets
        .get(app.selected)
        .map(|ticket| format!("Actions ({})", ticket.id.as_str()))
        .unwrap_or_else(|| "Actions".to_string());
    let block = Block::default().borders(Borders::ALL).title(title);
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    let items: Vec<ListItem> = state
        .items
        .iter()
        .map(|item| {
            let label = if item.enabled {
                format!("[{}] {}", item.hotkey, item.label)
            } else {
                format!("[{}] {} (disabled)", item.hotkey, item.label)
            };
            let style = if item.enabled {
                Style::default()
            } else {
                disabled_style(app.no_color)
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let mut list_state = ListState::default();
    if !state.items.is_empty() {
        list_state.select(Some(state.selected));
    }
    let list = List::new(items)
        .highlight_symbol(">> ")
        .highlight_style(highlight_style(app.no_color));
    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    let help = Paragraph::new("Enter to run | Esc to close")
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(help, chunks[1]);
}

/// Draw search overlay.
pub fn draw_search_overlay(frame: &mut Frame, state: &SearchState, app: &App) {
    let area = centered_rect(70, 30, frame.size());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Search tickets");
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let match_info = if state.match_indices.is_empty() {
        format!("Matches: {}", app.tickets.len())
    } else {
        format!(
            "Match {}/{} (n/N to navigate)",
            state.current_match + 1,
            state.match_indices.len()
        )
    };

    let prompt = Line::from(vec![Span::raw("Query: "), Span::raw(state.query.as_str())]);
    let hint = Line::from("Matches id, title, summary, tags, assignees.");
    let matches = Line::from(match_info);
    let help = Line::from("Enter to apply | Esc to cancel");
    let paragraph =
        Paragraph::new(vec![prompt, Line::from(""), hint, matches, help]).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);

    let cursor_x = inner.x + 7 + state.cursor as u16;
    let cursor_y = inner.y;
    let max_x = inner.x + inner.width.saturating_sub(1);
    frame.set_cursor(cursor_x.min(max_x), cursor_y);
}

/// Draw selection menu overlay.
pub fn draw_select_overlay(frame: &mut Frame, state: &SelectState, app: &App) {
    let area = centered_rect(50, 40, frame.size());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(state.title.as_str());
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let items: Vec<ListItem> = state
        .options
        .iter()
        .map(|opt| {
            let label = if let Some(key) = opt.hotkey {
                format!("[{}] {}", key, opt.label)
            } else {
                opt.label.clone()
            };
            ListItem::new(Line::from(label))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));

    let list = List::new(items)
        .highlight_symbol(">> ")
        .highlight_style(highlight_style(app.no_color));
    frame.render_stateful_widget(list, inner, &mut list_state);
}

/// Draw help overlay.
pub fn draw_help_overlay(frame: &mut Frame, app: &App) {
    let area = centered_rect(90, 90, frame.size());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Help - Press ? or Esc to close");
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let key_style = key_hint_style(app.no_color);
    let header_style = Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let symbol_style = |color| {
        if app.no_color {
            Style::default()
        } else {
            Style::default().fg(color)
        }
    };

    let lines = vec![
        // Navigation
        Line::from(Span::styled("NAVIGATION", header_style)),
        Line::from(vec![
            Span::styled("j/↓", key_style),
            Span::raw(" down  "),
            Span::styled("k/↑", key_style),
            Span::raw(" up  "),
            Span::styled("gg", key_style),
            Span::raw(" top  "),
            Span::styled("G", key_style),
            Span::raw(" bottom  "),
            Span::styled("Ctrl+d/u", key_style),
            Span::raw(" half-page"),
        ]),
        Line::from(vec![
            Span::styled("h/Esc", key_style),
            Span::raw(" left panel  "),
            Span::styled("l/Enter", key_style),
            Span::raw(" right panel"),
        ]),
        Line::from(""),
        // Details Panel
        Line::from(Span::styled("DETAILS PANEL (right side)", header_style)),
        Line::from(vec![
            Span::styled("Tab/→/]", key_style),
            Span::raw(" next tab  "),
            Span::styled("Shift+Tab/←/[", key_style),
            Span::raw(" prev tab  "),
            Span::styled("1-5", key_style),
            Span::raw(" jump"),
        ]),
        Line::from(vec![
            Span::raw("Tabs: "),
            Span::styled("1", key_style),
            Span::raw(" Info  "),
            Span::styled("2", key_style),
            Span::raw(" Notes  "),
            Span::styled("3", key_style),
            Span::raw(" History  "),
            Span::styled("4", key_style),
            Span::raw(" Relations  "),
            Span::styled("5", key_style),
            Span::raw(" Diff"),
        ]),
        Line::from(""),
        // Actions
        Line::from(Span::styled("TICKET ACTIONS", header_style)),
        Line::from(vec![
            Span::styled("n", key_style),
            Span::raw(" new ticket  "),
            Span::styled("a", key_style),
            Span::raw(" add note  "),
            Span::styled("c", key_style),
            Span::raw(" close  "),
            Span::styled("r", key_style),
            Span::raw(" reopen"),
        ]),
        Line::from(vec![
            Span::styled("p", key_style),
            Span::raw(" priority  "),
            Span::styled("v", key_style),
            Span::raw(" severity  "),
            Span::styled("s", key_style),
            Span::raw(" status  "),
            Span::styled("t", key_style),
            Span::raw(" tags"),
        ]),
        Line::from(vec![
            Span::styled("U", key_style),
            Span::raw(" assignees  "),
            Span::styled("M", key_style),
            Span::raw(" milestone  "),
            Span::styled("e", key_style),
            Span::raw(" edit  "),
            Span::styled("o", key_style),
            Span::raw(" edit ($EDITOR)  "),
            Span::styled("E", key_style),
            Span::raw(" edit notes"),
        ]),
        Line::from(""),
        // Search & Filter
        Line::from(Span::styled("SEARCH & FILTER", header_style)),
        Line::from(vec![
            Span::styled("/", key_style),
            Span::raw(" search  "),
            Span::styled("x", key_style),
            Span::raw(" clear search  "),
            Span::styled("f", key_style),
            Span::raw(" cycle filter  "),
            Span::styled("F", key_style),
            Span::raw(" filter menu  "),
            Span::styled("R", key_style),
            Span::raw(" refresh"),
        ]),
        Line::from(""),
        // Relations (in Relations tab)
        Line::from(Span::styled("RELATIONS TAB", header_style)),
        Line::from(vec![
            Span::styled("+", key_style),
            Span::raw(" add relation  "),
            Span::styled("d", key_style),
            Span::raw(" delete relation  "),
            Span::styled("Enter", key_style),
            Span::raw(" jump to target"),
        ]),
        Line::from(""),
        // Diff tab
        Line::from(Span::styled("DIFF TAB (press 5)", header_style)),
        Line::from(vec![
            Span::styled("</>", key_style),
            Span::raw(" change versions  "),
            Span::styled("j/k", key_style),
            Span::raw(" scroll diff"),
        ]),
        Line::from(""),
        // Other
        Line::from(Span::styled("OTHER", header_style)),
        Line::from(vec![
            Span::styled("?", key_style),
            Span::raw(" this help  "),
            Span::styled("m", key_style),
            Span::raw(" action menu  "),
            Span::styled("D", key_style),
            Span::raw(" dependency graph  "),
            Span::styled("q", key_style),
            Span::raw(" quit"),
        ]),
        Line::from(""),
        // Status Symbols Legend
        Line::from(Span::styled("STATUS SYMBOLS", header_style)),
        Line::from(vec![
            Span::styled("○", symbol_style(Color::Green)),
            Span::raw(" Open  "),
            Span::styled("◐", symbol_style(Color::Yellow)),
            Span::raw(" In Progress  "),
            Span::styled("⊘", symbol_style(Color::Red)),
            Span::raw(" Blocked  "),
            Span::styled("●", symbol_style(Color::Gray)),
            Span::raw(" Closed  "),
            Span::styled("▣", symbol_style(Color::DarkGray)),
            Span::raw(" Archived"),
        ]),
        Line::from(""),
        // Priority Symbols Legend
        Line::from(Span::styled("PRIORITY SYMBOLS", header_style)),
        Line::from(vec![
            Span::styled("!!", symbol_style(Color::Red)),
            Span::raw(" Critical  "),
            Span::styled("!", symbol_style(Color::Yellow)),
            Span::raw(" High  "),
            Span::styled("·", symbol_style(Color::Gray)),
            Span::raw(" Medium  "),
            Span::raw("  Low (no symbol)"),
        ]),
        Line::from(""),
        // Focus Indicators Legend
        Line::from(Span::styled("FOCUS INDICATORS", header_style)),
        Line::from(vec![
            Span::styled("◆", symbol_style(Color::Cyan)),
            Span::raw(" Focused panel (can interact)  "),
            Span::styled("○", symbol_style(Color::DarkGray)),
            Span::raw(" Unfocused panel"),
        ]),
        Line::from("Use Tab, h, or l to switch between panels."),
    ];

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

/// Draw milestone selection overlay.
pub fn draw_milestone_overlay(frame: &mut Frame, state: &MilestoneMenuState, app: &App) {
    let area = centered_rect(60, 50, frame.size());
    frame.render_widget(Clear, area);
    let title = if state.ticket_ids.len() > 1 {
        format!("Set Milestone ({} tickets)", state.ticket_ids.len())
    } else {
        "Set Milestone".to_string()
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let mut items: Vec<ListItem> = vec![ListItem::new(Line::from("[x] Clear milestone"))];

    for milestone in &state.milestones {
        items.push(ListItem::new(Line::from(format!(
            "    {} - {}",
            milestone.id.as_str(),
            milestone.title
        ))));
    }

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));

    let list = List::new(items)
        .highlight_symbol(">> ")
        .highlight_style(highlight_style(app.no_color));
    frame.render_stateful_widget(list, inner, &mut list_state);
}

/// Draw tag edit overlay.
pub fn draw_tag_edit_overlay(frame: &mut Frame, state: &TagEditState, app: &App) {
    let area = centered_rect(70, 50, frame.size());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!("Edit Tags - {}", state.ticket_id));
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let tag_style = badge_style(app.no_color);
    let hint_style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let suggestion_style = if app.no_color {
        Style::default().add_modifier(Modifier::DIM)
    } else {
        Style::default().fg(Color::Cyan)
    };
    let selected_suggestion_style = if app.no_color {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    };

    let mut lines: Vec<Line> = Vec::new();

    // Current tags line
    let mut tag_spans: Vec<Span> = vec![Span::raw("Tags: ")];
    if state.tags.is_empty() {
        tag_spans.push(Span::styled("(none)", hint_style));
    } else {
        for (i, tag) in state.tags.iter().enumerate() {
            if i > 0 {
                tag_spans.push(Span::raw(" "));
            }
            tag_spans.push(Span::styled(format!(" {} ", tag), tag_style));
        }
    }
    lines.push(Line::from(tag_spans));
    lines.push(Line::from(""));

    // Input line
    let input_line = Line::from(vec![
        Span::raw("Add: "),
        Span::raw(state.input.as_str()),
        Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
    ]);
    lines.push(input_line);

    // Suggestions (if any)
    if !state.suggestions.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Suggestions:", hint_style)));

        for (i, suggestion) in state.suggestions.iter().enumerate() {
            let is_selected = state.suggestion_idx == Some(i);
            let marker = if is_selected { ">> " } else { "   " };
            let style = if is_selected {
                selected_suggestion_style
            } else {
                suggestion_style
            };
            lines.push(Line::from(vec![
                Span::raw(marker),
                Span::styled(suggestion.clone(), style),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Type to add • Backspace to remove • Tab to cycle suggestions • Enter to save • Esc to cancel",
        hint_style,
    )));

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);

    // Set cursor position
    let cursor_x = inner.x + 5 + state.cursor as u16;
    let cursor_y = inner.y + 2;
    let max_x = inner.x + inner.width.saturating_sub(1);
    frame.set_cursor(cursor_x.min(max_x), cursor_y);
}

/// Draw relation edit overlay.
pub fn draw_relation_edit_overlay(frame: &mut Frame, state: &RelationEditState, app: &App) {
    let area = centered_rect(70, 60, frame.size());
    frame.render_widget(Clear, area);
    let title = match state.step {
        RelationStep::SelectType => format!("Add Relation - {} (Step 1/2)", state.ticket_id),
        RelationStep::SearchTarget => format!(
            "Add Relation - {} {} (Step 2/2)",
            state.relation_type.as_ref().map(|r| r.as_str()).unwrap_or(""),
            state.ticket_id
        ),
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let hint_style = if app.no_color {
        Style::default()
    } else {
        Style::default().fg(Color::DarkGray)
    };

    match state.step {
        RelationStep::SelectType => {
            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from("Select relation type:"));
            lines.push(Line::from(""));

            for (i, rel_type) in state.relation_types.iter().enumerate() {
                let is_selected = i == state.type_selected;
                let marker = if is_selected { ">> " } else { "   " };
                let style = if is_selected {
                    highlight_style(app.no_color)
                } else {
                    Style::default()
                };
                lines.push(Line::from(vec![
                    Span::styled(marker, style),
                    Span::styled(rel_type.as_str().to_string(), style),
                ]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "j/k to move • Enter to select • Esc to cancel",
                hint_style,
            )));

            let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
            frame.render_widget(paragraph, inner);
        }
        RelationStep::SearchTarget => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Search input
                    Constraint::Min(0),    // Ticket list
                    Constraint::Length(1), // Hint
                ])
                .split(inner);

            // Search input
            let search_line = Line::from(vec![
                Span::raw("Search: "),
                Span::raw(state.search_query.as_str()),
                Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
            ]);
            let match_count = format!(" ({} matches)", state.filtered_tickets.len());
            let search_block = Paragraph::new(vec![
                search_line,
                Line::from(Span::styled(match_count, hint_style)),
            ]);
            frame.render_widget(search_block, chunks[0]);

            // Ticket list
            let items: Vec<ListItem> = state
                .filtered_tickets
                .iter()
                .enumerate()
                .take(chunks[1].height as usize)
                .map(|(i, ticket)| {
                    let is_selected = i == state.ticket_selected;
                    let marker = if is_selected { ">> " } else { "   " };
                    let status_sym = status_symbol(&ticket.status);
                    let style = if is_selected {
                        highlight_style(app.no_color)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(marker, style),
                        Span::styled(status_sym.to_string(), status_style(&ticket.status, app.no_color)),
                        Span::raw(" "),
                        Span::styled(format!("{} ", ticket.id), style),
                        Span::styled(ticket.title.clone(), style),
                    ]))
                })
                .collect();

            let list = List::new(items);
            frame.render_widget(list, chunks[1]);

            // Hint
            let hint = Paragraph::new(Span::styled(
                "Type to filter • j/k to move • Enter to add • Esc to go back",
                hint_style,
            ));
            frame.render_widget(hint, chunks[2]);

            // Set cursor position
            let cursor_x = chunks[0].x + 8 + state.search_cursor as u16;
            let cursor_y = chunks[0].y;
            let max_x = chunks[0].x + chunks[0].width.saturating_sub(1);
            frame.set_cursor(cursor_x.min(max_x), cursor_y);
        }
    }
}

/// Draw graph view overlay.
pub fn draw_graph_overlay(frame: &mut Frame, state: &GraphState, app: &App) {
    let area = centered_rect(90, 90, frame.size());
    frame.render_widget(Clear, area);

    let node_count = state.graph.nodes.len();
    let edge_count = state.graph.edges.len();
    let title = format!(
        "Dependency Graph ({} nodes, {} edges)",
        node_count, edge_count
    );

    let block = Block::default().borders(Borders::ALL).title(title);
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let hint_style = if app.no_color {
        Style::default().add_modifier(Modifier::DIM)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let highlight_style = if app.no_color {
        Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    };

    let edge_style = if app.no_color {
        Style::default().add_modifier(Modifier::DIM)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    // Split area for graph content and footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(inner);

    // Render graph lines
    let visible_height = chunks[0].height as usize;
    let mut node_idx = 0;

    let visible_lines: Vec<Line> = state
        .rendered_lines
        .iter()
        .skip(state.scroll_offset)
        .take(visible_height)
        .map(|line| {
            let is_focused = if line.node_id.is_some() {
                let focused = node_idx == state.focused_node;
                node_idx += 1;
                focused
            } else {
                false
            };

            let style = if is_focused {
                highlight_style
            } else if line.is_edge {
                edge_style
            } else {
                Style::default()
            };

            // Add focus marker for focused line
            let marker = if is_focused { ">> " } else { "   " };

            Line::from(vec![
                Span::styled(marker, style),
                Span::styled(line.content.clone(), style),
            ])
        })
        .collect();

    let graph_content = Paragraph::new(visible_lines).wrap(Wrap { trim: false });
    frame.render_widget(graph_content, chunks[0]);

    // Footer with hints
    let hints = vec![
        Line::from(""),
        Line::from(Span::styled(
            "j/k: navigate • Enter: jump to ticket • Ctrl+d/u: scroll • q/Esc: close",
            hint_style,
        )),
    ];
    let footer = Paragraph::new(hints);
    frame.render_widget(footer, chunks[1]);
}

/// Create a ticket list item with proper column alignment.
fn ticket_list_item<'a>(ticket: &'a Ticket, no_color: bool, is_selected: bool) -> ListItem<'a> {
    let priority_sym = priority_symbol(&ticket.priority);
    let status_sym = status_symbol(&ticket.status);
    // Fixed-width columns: selection(2) + priority(2) + space(1) + status_sym(1) + space(1) + status_text(11) + space(1)
    let status_col = format!("{} {:<11}", status_sym, ticket.status.as_str());

    // Selection marker
    let selection_marker = if is_selected { "◉ " } else { "  " };
    let selection_style = if no_color {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    };

    let line = Line::from(vec![
        // Selection marker column: 2 chars
        Span::styled(
            selection_marker,
            if is_selected {
                selection_style
            } else {
                Style::default()
            },
        ),
        // Priority column: 2 chars + 1 space separator
        Span::styled(
            format!("{:<2} ", priority_sym),
            priority_style(&ticket.priority, no_color),
        ),
        // Status column: symbol + text (styled together to avoid color gaps)
        Span::styled(status_col, status_style(&ticket.status, no_color)),
        // Separator
        Span::raw(" "),
        // ID column (fixed width for ULIDs)
        Span::raw(format!("{:<26} ", ticket.id.as_str())),
        // Title (rest of space)
        Span::raw(ticket.title.as_str()),
    ]);
    ListItem::new(line)
}

/// Join items with comma or return dash.
fn join_or_dash(items: &[String]) -> String {
    if items.is_empty() {
        "-".to_string()
    } else {
        items.join(", ")
    }
}

/// Create a centered rectangle.
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::{Action, ActionItem, ConfirmAction};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tempfile::tempdir;
    use tik_core::{
        ArtifactType, Milestone, NewMilestone, NewTicket, RelationType, Repo, Ticket,
        TicketStatus,
    };

    fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
        let mut output = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                output.push_str(buffer.get(x, y).symbol());
            }
            output.push('\n');
        }
        output
    }

    fn render_to_string<F>(width: u16, height: u16, render: F) -> String
    where
        F: FnOnce(&mut Frame),
    {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(render).unwrap();
        buffer_to_string(terminal.backend().buffer())
    }

    fn render_app(app: &App) -> String {
        render_to_string(100, 30, |frame| draw(frame, app))
    }

    #[test]
    fn welcome_screen_includes_hint() {
        let dir = tempdir().unwrap();
        let app = App::new(dir.path().to_path_buf(), None, true).unwrap();
        let output = render_app(&app);
        assert!(output.contains("No .tik repo found in this directory."));
        assert!(output.contains("Press i to initialize here, or q to quit."));
    }

    #[test]
    fn draw_tabs_render_details_content() {
        let dir = tempdir().unwrap();
        let repo = Repo::init(dir.path(), "0.1.0-test").unwrap();
        let ticket = repo
            .create_ticket(
                NewTicket {
                    title: "Add coverage".to_string(),
                    summary: Some("Summary text".to_string()),
                    description: Some("Description text".to_string()),
                    tags: vec!["ui".to_string()],
                },
                "tester",
            )
            .unwrap();
        let related = repo
            .create_ticket(
                NewTicket {
                    title: "Related".to_string(),
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
        repo.add_relation(
            &ticket.id,
            RelationType::Blocks,
            &related.id,
            "tester",
            Some("rel"),
        )
        .unwrap();
        repo.add_artifact(
            &ticket.id,
            ArtifactType::File,
            "docs/spec.md",
            "tester",
            Some("artifact"),
        )
        .unwrap();
        repo.set_ticket_milestone(&ticket.id, Some(&milestone.id), "tester", Some("ms"))
            .unwrap();
        repo.append_note(&ticket.id, "tester", "Note text").unwrap();
        repo.update_status(&ticket.id, TicketStatus::InProgress, "tester", Some("progress"))
            .unwrap();

        let mut app = App::new(dir.path().to_path_buf(), Some(repo), true).unwrap();
        app.focused_panel = Panel::Details;

        app.details_tab = DetailsTab::Info;
        let info = render_app(&app);
        assert!(info.contains("Tickets"));
        assert!(info.contains("Details"));
        assert!(info.contains("Add coverage"));
        assert!(info.contains("Summary:"));
        assert!(info.contains("Description:"));

        app.details_tab = DetailsTab::Notes;
        let notes = render_app(&app);
        assert!(notes.contains("Note text"));

        app.details_tab = DetailsTab::History;
        let history = render_app(&app);
        assert!(history.contains("status_change"));

        app.details_tab = DetailsTab::Relations;
        let relations = render_app(&app);
        assert!(relations.contains("Relations:"));
        assert!(relations.contains("Artifacts:"));
        assert!(relations.contains("Milestone:"));
        assert!(relations.contains("blocks"));
        assert!(relations.contains("Related"));
        assert!(relations.contains("[open] Milestone"));
        assert!(relations.contains("docs/spec.md"));
    }

    #[test]
    fn overlays_render_expected_labels() {
        let dir = tempdir().unwrap();
        let mut app = App::new(dir.path().to_path_buf(), None, true).unwrap();
        let ticket = Ticket::new(
            NewTicket {
                title: "Ticket One".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        app.tickets = vec![ticket.clone()];
        app.selected = 0;

        let input_state = InputState::new_ticket();
        let input = render_to_string(80, 20, |frame| draw_input_overlay(frame, &input_state));
        assert!(input.contains("New Ticket"));

        let confirm_state = ConfirmState {
            prompt: "Confirm?".to_string(),
            action: ConfirmAction::InitRepo {
                setup_claude: false,
                setup_agents: false,
            },
        };
        let confirm = render_to_string(60, 20, |frame| draw_confirm_overlay(frame, &confirm_state));
        assert!(confirm.contains("Confirm?"));

        let action_state = ActionState {
            items: vec![
                ActionItem {
                    hotkey: 'n',
                    label: "New Ticket".to_string(),
                    enabled: true,
                    action: Action::NewTicket,
                },
                ActionItem {
                    hotkey: 'x',
                    label: "Disabled".to_string(),
                    enabled: false,
                    action: Action::Quit,
                },
            ],
            selected: 0,
        };
        let actions =
            render_to_string(80, 20, |frame| draw_action_overlay(frame, &action_state, &app));
        assert!(actions.contains("Actions"));

        let mut search_state = SearchState::new("query".to_string(), None);
        let search_empty =
            render_to_string(80, 20, |frame| draw_search_overlay(frame, &search_state, &app));
        assert!(search_empty.contains("Matches:"));
        search_state.match_indices = vec![0];
        search_state.current_match = 0;
        let search =
            render_to_string(80, 20, |frame| draw_search_overlay(frame, &search_state, &app));
        assert!(search.contains("Match 1/1"));

        let select_state = SelectState::priority(ticket.id.as_str().to_string(), Vec::new());
        let select =
            render_to_string(80, 20, |frame| draw_select_overlay(frame, &select_state, &app));
        assert!(select.contains("Select Priority"));

        let help = render_to_string(100, 30, |frame| draw_help_overlay(frame, &app));
        assert!(help.contains("Help"));

        let milestone = Milestone::new(
            NewMilestone {
                title: "Milestone".to_string(),
                description: None,
                due_at: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        let milestone_state = MilestoneMenuState {
            ticket_ids: vec![ticket.id.as_str().to_string()],
            milestones: vec![milestone],
            selected: 0,
        };
        let milestone_output = render_to_string(80, 20, |frame| {
            draw_milestone_overlay(frame, &milestone_state, &app);
        });
        assert!(milestone_output.contains("Set Milestone"));

        let mut tag_state = TagEditState::new(
            ticket.id.as_str().to_string(),
            vec!["ui".to_string()],
            vec!["ui".to_string(), "ux".to_string()],
        );
        tag_state.input = "u".to_string();
        tag_state.update_suggestions();
        let tag_output =
            render_to_string(100, 25, |frame| draw_tag_edit_overlay(frame, &tag_state, &app));
        assert!(tag_output.contains("Edit Tags"));
        assert!(tag_output.contains("Suggestions:"));
    }

    #[test]
    fn helpers_build_expected_strings() {
        let mut app = App::new(tempdir().unwrap().path().to_path_buf(), None, true).unwrap();
        app.status = "ready".to_string();
        let status_line = build_status_line(&app);
        let buffer = ratatui::buffer::Buffer::with_lines([status_line]);
        let output = buffer_to_string(&buffer);
        assert!(output.contains("ready"));

        let hints_line = build_hints_line(&app);
        let buffer = ratatui::buffer::Buffer::with_lines([hints_line]);
        let output = buffer_to_string(&buffer);
        assert!(output.contains("quit"));

        let centered = centered_rect(50, 50, Rect::new(0, 0, 100, 40));
        assert_eq!(centered.width, 50);
        assert_eq!(centered.height, 20);
    }
}
