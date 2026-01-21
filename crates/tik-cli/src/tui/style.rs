//! TUI styling helpers.

use ratatui::style::{Color, Modifier, Style};
use tik_core::{Priority, TicketStatus};

/// Style for ticket status.
pub fn status_style(status: &TicketStatus, no_color: bool) -> Style {
    if no_color {
        return Style::default();
    }
    match status {
        TicketStatus::Open => Style::default().fg(Color::Green),
        TicketStatus::InProgress => Style::default().fg(Color::Yellow),
        TicketStatus::Blocked => Style::default().fg(Color::Red),
        TicketStatus::Closed => Style::default().fg(Color::Gray),
        TicketStatus::Archived => Style::default().fg(Color::DarkGray),
    }
}

/// Highlighted item style.
pub fn highlight_style(no_color: bool) -> Style {
    if no_color {
        Style::default()
    } else {
        Style::default().add_modifier(Modifier::REVERSED)
    }
}

/// Disabled item style.
pub fn disabled_style(no_color: bool) -> Style {
    if no_color {
        Style::default()
    } else {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM)
    }
}

/// Style for focused panel border.
pub fn focused_border_style(no_color: bool) -> Style {
    if no_color {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    }
}

/// Style for unfocused panel border.
pub fn unfocused_border_style(no_color: bool) -> Style {
    if no_color {
        Style::default()
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

/// Style for priority indicator.
pub fn priority_style(priority: &Priority, no_color: bool) -> Style {
    if no_color {
        return Style::default();
    }
    match priority {
        Priority::Critical => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        Priority::High => Style::default().fg(Color::Yellow),
        Priority::Medium => Style::default().fg(Color::Gray),
        Priority::Low => Style::default().fg(Color::DarkGray),
    }
}

/// Priority indicator symbol (single char, padded in rendering).
pub fn priority_symbol(priority: &Priority) -> &'static str {
    match priority {
        Priority::Critical => "!!",
        Priority::High => "!",
        Priority::Medium => "·",
        Priority::Low => " ",
    }
}

/// Status indicator symbol.
pub fn status_symbol(status: &TicketStatus) -> &'static str {
    match status {
        TicketStatus::Open => "○",
        TicketStatus::InProgress => "◐",
        TicketStatus::Blocked => "⊘",
        TicketStatus::Closed => "●",
        TicketStatus::Archived => "▣",
    }
}

/// Style for active tab.
pub fn active_tab_style(no_color: bool) -> Style {
    if no_color {
        Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }
}

/// Style for inactive tab.
pub fn inactive_tab_style(no_color: bool) -> Style {
    if no_color {
        Style::default()
    } else {
        Style::default().fg(Color::Gray)
    }
}

/// Style for key hints in footer.
pub fn key_hint_style(no_color: bool) -> Style {
    if no_color {
        Style::default()
    } else {
        Style::default().fg(Color::Yellow)
    }
}

/// Style for filter badge.
pub fn badge_style(no_color: bool) -> Style {
    if no_color {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    }
}

/// Style for event type in history.
pub fn event_type_style(event_type: &str, no_color: bool) -> Style {
    if no_color {
        return Style::default();
    }
    match event_type {
        "created" => Style::default().fg(Color::Green),
        "status_change" => Style::default().fg(Color::Yellow),
        "note" => Style::default().fg(Color::Blue),
        "edited" => Style::default().fg(Color::Magenta),
        "assigned" => Style::default().fg(Color::Cyan),
        "tagged" => Style::default().fg(Color::Gray),
        _ => Style::default(),
    }
}

/// Icon for event type.
pub fn event_icon(event_type: &str) -> &'static str {
    match event_type {
        "created" => "+",
        "status_change" => "~",
        "note" => "#",
        "edited" => "*",
        "assigned" => "@",
        "tagged" => "#",
        "relation_added" => ">",
        "relation_removed" => "<",
        "artifact_added" => "^",
        "artifact_removed" => "v",
        "milestone_set" => "M",
        "milestone_cleared" => "m",
        _ => "·",
    }
}
