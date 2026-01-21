//! TUI rendering functions.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap};
use ratatui::Frame;
use tik_core::Ticket;

use super::app::App;
use super::state::{
    ActionState, ConfirmState, DetailsTab, InputState, MilestoneMenuState, Panel, SearchState,
    SelectState, TagEditState,
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
    } else {
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
    }

    match &app.mode {
        super::state::Mode::Input(state) => draw_input_overlay(frame, state),
        super::state::Mode::Confirm(state) => draw_confirm_overlay(frame, state),
        super::state::Mode::Action(state) => draw_action_overlay(frame, state, app),
        super::state::Mode::Search(state) => draw_search_overlay(frame, state, app),
        super::state::Mode::Select(state) => draw_select_overlay(frame, state, app),
        super::state::Mode::Help => draw_help_overlay(frame, app),
        super::state::Mode::MilestoneMenu(state) => draw_milestone_overlay(frame, state, app),
        super::state::Mode::TagEdit(state) => draw_tag_edit_overlay(frame, state, app),
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

    // Calculate scroll indicators
    let visible_height = area.height.saturating_sub(2) as usize; // subtract borders
    let total_items = app.tickets.len();
    let can_scroll_up = app.selected > 0;
    let can_scroll_down = app.selected + 1 < total_items && total_items > visible_height;

    let scroll_indicator = match (can_scroll_up, can_scroll_down) {
        (true, true) => " ↕",
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

    let mut lines = Vec::new();

    // Relations
    lines.push(Line::from("Relations:"));
    if ticket.relations.is_empty() {
        lines.push(Line::from("  (none)"));
    } else {
        for rel in &ticket.relations {
            lines.push(Line::from(format!(
                "  {} {}",
                rel.kind.as_str(),
                rel.id.as_str()
            )));
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
                "  [{}] {}",
                artifact.kind.as_str(),
                artifact.reference
            )));
        }
    }

    lines.push(Line::from(""));

    // Milestone
    lines.push(Line::from("Milestone:"));
    if let Some(ref milestone_id) = ticket.milestone_id {
        lines.push(Line::from(format!("  {}", milestone_id.as_str())));
    } else {
        lines.push(Line::from("  (none)"));
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
            super::state::Mode::Action(_) => "ACTION",
            super::state::Mode::Search(_) => "SEARCH",
            super::state::Mode::Select(_) => "SELECT",
            super::state::Mode::Help => "HELP",
            super::state::Mode::MilestoneMenu(_) => "MILESTONE",
            super::state::Mode::TagEdit(_) => "TAGS",
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
        height: area.height.saturating_sub(2),
    };
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
            Span::styled("Tab", key_style),
            Span::raw(" switch panel  "),
            Span::styled("h", key_style),
            Span::raw(" left panel  "),
            Span::styled("l", key_style),
            Span::raw(" right panel"),
        ]),
        Line::from(""),
        // Details Panel
        Line::from(Span::styled("DETAILS PANEL (right side)", header_style)),
        Line::from(vec![
            Span::styled("←/[", key_style),
            Span::raw(" prev tab  "),
            Span::styled("→/]", key_style),
            Span::raw(" next tab"),
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
        // Other
        Line::from(Span::styled("OTHER", header_style)),
        Line::from(vec![
            Span::styled("?", key_style),
            Span::raw(" this help  "),
            Span::styled("m", key_style),
            Span::raw(" action menu  "),
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
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Set Milestone");
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
