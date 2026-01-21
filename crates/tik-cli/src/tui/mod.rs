//! Tiketer TUI module - a lazygit-inspired terminal interface.
//!
//! Features:
//! - Panel focus system with colored borders
//! - Vim-style navigation (j/k, gg, G, Ctrl+D/U)
//! - Tab system in details panel (Info, Notes, History, Relations)
//! - Selection menus for enum fields
//! - Context-sensitive footer hints
//! - Search with match navigation
//! - $EDITOR integration for editing tickets and notes

mod app;
mod input;
mod render;
mod state;
mod style;

use std::io::{self, Write};
use std::process::Command;
use std::time::Duration;

use crossterm::event::{self, Event};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tik_core::{Repo, Result, TikError};

use app::App;

/// Run the TUI application.
pub fn run_tui(no_color: bool, project: Option<&str>) -> Result<()> {
    let root = std::env::current_dir().map_err(|err| TikError::io("get current dir", err))?;
    let repo = match Repo::discover_with_project(&root, project) {
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

        // Check for pending editor launch
        if let Some(target) = app.take_pending_editor() {
            if let Some(path) = app.editor_target_path(&target) {
                // Suspend TUI
                tui.suspend()?;

                // Launch editor
                let result = launch_editor(&path);

                // Resume TUI
                tui.resume()?;

                match result {
                    Ok(()) => {
                        if let Err(err) = app.after_editor(&target) {
                            app.status = format!("error: {err}");
                        }
                    }
                    Err(err) => {
                        app.status = format!("editor error: {err}");
                    }
                }
            }
        }
    }
    Ok(())
}

/// Get the editor command from environment.
fn get_editor() -> String {
    std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string())
}

/// Launch the editor for a file path.
fn launch_editor(path: &std::path::Path) -> Result<()> {
    let editor = get_editor();

    // Parse editor command (might have arguments like "code --wait")
    let mut parts = editor.split_whitespace();
    let program = parts.next().ok_or_else(|| TikError::usage("empty EDITOR"))?;
    let args: Vec<&str> = parts.collect();

    let mut cmd = Command::new(program);
    cmd.args(&args);
    cmd.arg(path);

    let status = cmd
        .status()
        .map_err(|err| TikError::io(&format!("spawn editor '{editor}'"), err))?;

    if !status.success() {
        return Err(TikError::usage(&format!(
            "editor exited with status: {}",
            status
        )));
    }

    Ok(())
}

/// Terminal wrapper with cleanup on drop.
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

    /// Suspend the TUI for external command execution (e.g., $EDITOR).
    fn suspend(&mut self) -> Result<()> {
        disable_raw_mode().map_err(|err| TikError::io("disable raw mode", err))?;
        io::stdout()
            .execute(LeaveAlternateScreen)
            .map_err(|err| TikError::io("leave alternate screen", err))?;
        // Flush stdout to ensure the screen is properly left
        io::stdout()
            .flush()
            .map_err(|err| TikError::io("flush stdout", err))?;
        Ok(())
    }

    /// Resume the TUI after external command execution.
    fn resume(&mut self) -> Result<()> {
        enable_raw_mode().map_err(|err| TikError::io("enable raw mode", err))?;
        io::stdout()
            .execute(EnterAlternateScreen)
            .map_err(|err| TikError::io("enter alternate screen", err))?;
        // Clear and redraw the terminal
        self.terminal
            .clear()
            .map_err(|err| TikError::io("clear terminal", err))?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::state::*;

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
