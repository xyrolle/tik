use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use tik_core::{NewTicket, Repo, Result, Ticket, TicketId, TicketStatus, TikError};

pub fn run_tui(no_color: bool) -> Result<()> {
    let root = std::env::current_dir().map_err(|err| TikError::io("get current dir", err))?;
    let repo = match Repo::discover(&root) {
        Ok(repo) => Some(repo),
        Err(TikError::RepoInvalid(_)) => None,
        Err(err) => return Err(err),
    };

    let mut app = App::new(root, repo, no_color)?;
    let mut tui = Tui::new()?;
    loop {
        tui.terminal
            .draw(|frame| app.draw(frame))
            .map_err(|err| TikError::io("draw terminal", err))?;
        if event::poll(Duration::from_millis(200))
            .map_err(|err| TikError::io("poll terminal", err))?
        {
            if let Event::Key(key) = event::read().map_err(|err| TikError::io("read key", err))? {
                if app.handle_key(key)? {
                    break;
                }
            }
        }
    }
    Ok(())
}

struct Tui {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl Tui {
    fn new() -> Result<Self> {
        enable_raw_mode().map_err(|err| TikError::io("enable raw mode", err))?;
        let mut stdout = io::stdout();
        stdout
            .execute(EnterAlternateScreen)
            .map_err(|err| TikError::io("enter alternate screen", err))?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(|err| TikError::io("init terminal", err))?;
        Ok(Self { terminal })
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = stdout.execute(LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

#[derive(Clone, Copy, Debug)]
enum TicketFilter {
    All,
    Open,
    InProgress,
    Blocked,
    Closed,
    Archived,
}

impl TicketFilter {
    fn next(self) -> Self {
        match self {
            TicketFilter::All => TicketFilter::Open,
            TicketFilter::Open => TicketFilter::InProgress,
            TicketFilter::InProgress => TicketFilter::Blocked,
            TicketFilter::Blocked => TicketFilter::Closed,
            TicketFilter::Closed => TicketFilter::Archived,
            TicketFilter::Archived => TicketFilter::All,
        }
    }

    fn label(self) -> &'static str {
        match self {
            TicketFilter::All => "all",
            TicketFilter::Open => "open",
            TicketFilter::InProgress => "in_progress",
            TicketFilter::Blocked => "blocked",
            TicketFilter::Closed => "closed",
            TicketFilter::Archived => "archived",
        }
    }

    fn status(self) -> Option<TicketStatus> {
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

#[derive(Debug)]
enum Mode {
    Normal,
    Input(InputState),
    Confirm(ConfirmState),
}

#[derive(Debug)]
enum InputKind {
    NewTicket,
    AddNote(String),
    CloseReason(String),
    ReopenReason(String),
}

#[derive(Debug)]
struct InputField {
    label: &'static str,
    value: String,
    required: bool,
}

#[derive(Debug)]
struct InputState {
    title: &'static str,
    kind: InputKind,
    fields: Vec<InputField>,
    current: usize,
    cursor: usize,
}

impl InputState {
    fn new_ticket() -> Self {
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

    fn add_note(id: String) -> Self {
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

    fn close_reason(id: String) -> Self {
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

    fn reopen_reason(id: String) -> Self {
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
}

#[derive(Debug)]
enum ConfirmAction {
    InitRepo,
    Close(String),
    Reopen(String),
}

#[derive(Debug)]
struct ConfirmState {
    prompt: String,
    action: ConfirmAction,
}

struct App {
    root: PathBuf,
    repo: Option<Repo>,
    tickets: Vec<Ticket>,
    selected: usize,
    filter: TicketFilter,
    mode: Mode,
    status: String,
    no_color: bool,
}

impl App {
    fn new(root: PathBuf, repo: Option<Repo>, no_color: bool) -> Result<Self> {
        let mut app = Self {
            root,
            repo,
            tickets: Vec::new(),
            selected: 0,
            filter: TicketFilter::All,
            mode: Mode::Normal,
            status: String::new(),
            no_color,
        };
        app.refresh(None)?;
        Ok(app)
    }

    fn refresh(&mut self, select_id: Option<&str>) -> Result<()> {
        let Some(repo) = &self.repo else {
            self.tickets.clear();
            return Ok(());
        };
        let tickets = repo.list_tickets(self.filter.status())?;
        self.tickets = tickets;
        if let Some(id) = select_id {
            if let Some(pos) = self
                .tickets
                .iter()
                .position(|ticket| ticket.id.as_str() == id)
            {
                self.selected = pos;
                return Ok(());
            }
        }
        if self.selected >= self.tickets.len() {
            self.selected = self.tickets.len().saturating_sub(1);
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        if self.repo.is_none() {
            draw_welcome(frame, self);
            return;
        }

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(2)])
            .split(frame.size());

        draw_header(frame, layout[0], self);
        draw_body(frame, layout[1], self);
        draw_footer(frame, layout[2], self);

        match &self.mode {
            Mode::Input(state) => draw_input_overlay(frame, state),
            Mode::Confirm(state) => draw_confirm_overlay(frame, state),
            Mode::Normal => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if matches!(&self.mode, Mode::Input(_)) {
            return self.handle_input_mode(key);
        }
        if matches!(&self.mode, Mode::Confirm(_)) {
            return self.handle_confirm_mode(key);
        }
        if matches!(key.code, KeyCode::Char('q')) {
            return Ok(true);
        }

        if self.repo.is_none() {
            return self.handle_uninitialized_key(key);
        }

        match key.code {
            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
            }
            KeyCode::Down => {
                if self.selected + 1 < self.tickets.len() {
                    self.selected += 1;
                }
            }
            KeyCode::Char('f') => {
                let current_id = self
                    .tickets
                    .get(self.selected)
                    .map(|ticket| ticket.id.as_str().to_string());
                self.filter = self.filter.next();
                self.refresh(current_id.as_deref())?;
            }
            KeyCode::Char('g') => {
                self.refresh(None)?;
            }
            KeyCode::Char('n') => {
                self.mode = Mode::Input(InputState::new_ticket());
            }
            KeyCode::Char('a') => {
                if let Some(ticket) = self.tickets.get(self.selected) {
                    self.mode = Mode::Input(InputState::add_note(ticket.id.as_str().to_string()));
                } else {
                    self.status = "no ticket selected".to_string();
                }
            }
            KeyCode::Char('c') => {
                if let Some(ticket) = self.tickets.get(self.selected) {
                    self.mode = Mode::Confirm(ConfirmState {
                        prompt: format!("Close {}?", ticket.id.as_str()),
                        action: ConfirmAction::Close(ticket.id.as_str().to_string()),
                    });
                }
            }
            KeyCode::Char('r') => {
                if let Some(ticket) = self.tickets.get(self.selected) {
                    self.mode = Mode::Confirm(ConfirmState {
                        prompt: format!("Reopen {}?", ticket.id.as_str()),
                        action: ConfirmAction::Reopen(ticket.id.as_str().to_string()),
                    });
                }
            }
            _ => {}
        }
        Ok(false)
    }

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

    fn perform_confirm_action(&mut self, action: ConfirmAction) -> Result<()> {
        match action {
            ConfirmAction::InitRepo => {
                let repo = Repo::init(&self.root, env!("CARGO_PKG_VERSION"))?;
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

    fn handle_input_mode(&mut self, key: KeyEvent) -> Result<bool> {
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
                    NewTicket {
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
                    if reason.is_empty() { None } else { Some(reason) },
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
                    if reason.is_empty() { None } else { Some(reason) },
                )?;
                self.refresh(Some(id))?;
                self.status = format!("reopened {id}");
                Ok(true)
            }
        }
    }
}

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

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let title = format!(
        "Tiketer TUI  |  repo: {}  |  filter: {}  |  {} tickets",
        app.root.display(),
        app.filter.label(),
        app.tickets.len()
    );
    let mut style = Style::default();
    if !app.no_color {
        style = style.add_modifier(Modifier::BOLD);
    }
    let paragraph = Paragraph::new(title)
        .style(style)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn draw_body(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    let items: Vec<ListItem> = app
        .tickets
        .iter()
        .map(|ticket| {
            let line = format!(
                "{} {:<12} {}",
                ticket.id.as_str(),
                ticket.status.as_str(),
                ticket.title
            );
            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default();
    if !app.tickets.is_empty() {
        state.select(Some(app.selected));
    }

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Tickets"))
        .highlight_symbol(">> ");
    frame.render_stateful_widget(list, chunks[0], &mut state);

    let details = app
        .tickets
        .get(app.selected)
        .map(format_ticket_details)
        .unwrap_or_else(|| "No ticket selected.".to_string());
    let paragraph = Paragraph::new(details)
        .block(Block::default().borders(Borders::ALL).title("Details"))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, chunks[1]);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let help = "q quit  n new  a note  c close  r reopen  f filter  g refresh";
    let text = if app.status.is_empty() {
        help.to_string()
    } else {
        format!("{}  |  {}", app.status, help)
    };
    let paragraph = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn draw_input_overlay(frame: &mut Frame, state: &InputState) {
    let area = centered_rect(80, 50, frame.size());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(state.title);
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

fn draw_confirm_overlay(frame: &mut Frame, state: &ConfirmState) {
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

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
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

fn format_ticket_details(ticket: &Ticket) -> String {
    format!(
        "ID: {}\nTitle: {}\nStatus: {}\nPriority: {}\nSeverity: {}\nUpdated: {}\n\nSummary:\n{}\n",
        ticket.id.as_str(),
        ticket.title,
        ticket.status.as_str(),
        ticket.priority.as_str(),
        ticket.severity.as_str(),
        ticket.updated_at,
        ticket.summary
    )
}

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

fn parse_csv_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect()
}

fn insert_char(text: &mut String, index: usize, ch: char) {
    let byte_idx = char_to_byte_index(text, index);
    text.insert(byte_idx, ch);
}

fn remove_char(text: &mut String, index: usize) {
    if index == 0 {
        return;
    }
    let start = char_to_byte_index(text, index - 1);
    let end = char_to_byte_index(text, index);
    text.replace_range(start..end, "");
}

fn char_to_byte_index(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
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
}
