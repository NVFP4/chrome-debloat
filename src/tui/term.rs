use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste};
use crossterm::execute;
use crossterm::style::Print;
use crossterm::terminal::{
    EnterAlternateScreen,
    LeaveAlternateScreen,
    disable_raw_mode,
    enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::{event, ui};
use crate::app::App;

pub type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;
const TICK_RATE: Duration = Duration::from_secs(1);
const ENABLE_ALTERNATE_SCROLL: &str = "\x1b[?1007h";
const DISABLE_ALTERNATE_SCROLL: &str = "\x1b[?1007l";

pub fn install_panic_hook() {
    let hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();

        let mut stdout = io::stdout();
        let _ = execute!(
            stdout,
            Print(DISABLE_ALTERNATE_SCROLL),
            LeaveAlternateScreen,
            DisableMouseCapture,
            DisableBracketedPaste
        );

        hook(panic_info);
    }));
}

pub fn init() -> Result<TuiTerminal> {
    enable_raw_mode().context("enable terminal raw mode")?;

    let mut stdout = io::stdout();
    if let Err(error) = execute!(
        stdout,
        EnterAlternateScreen,
        Print(ENABLE_ALTERNATE_SCROLL),
        EnableBracketedPaste
    ) {
        let _ = restore_stdout();
        return Err(error).context("enter alternate terminal screen");
    }

    let backend = CrosstermBackend::new(stdout);
    match Terminal::new(backend).context("create terminal") {
        Ok(terminal) => Ok(terminal),
        Err(error) => {
            let restore_result = restore_stdout();
            match restore_result {
                Ok(()) => Err(error),
                Err(restore_error) => {
                    Err(error.context(format!("terminal restore also failed: {restore_error}")))
                }
            }
        }
    }
}

pub fn run(mut terminal: TuiTerminal, app: &mut App) -> Result<()> {
    let run_result = run_loop(&mut terminal, app);
    let restore_result = restore(&mut terminal);

    match (run_result, restore_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(run_error), Err(restore_error)) => {
            Err(run_error.context(format!("terminal restore also failed: {restore_error}")))
        }
    }
}

fn run_loop(terminal: &mut TuiTerminal, app: &mut App) -> Result<()> {
    let mut needs_draw = true;

    while !app.should_quit() {
        if needs_draw {
            app.prepare_policy_view();
            terminal
                .draw(|frame| ui::render(frame, app))
                .context("draw terminal frame")?;
        }

        let action = event::read_action(app, TICK_RATE)?;
        needs_draw = app.handle_action(action);
    }

    Ok(())
}

fn restore(terminal: &mut TuiTerminal) -> Result<()> {
    let raw_mode_result = disable_raw_mode().context("disable terminal raw mode");
    let screen_result = execute!(
        terminal.backend_mut(),
        Print(DISABLE_ALTERNATE_SCROLL),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )
    .context("restore terminal screen");
    let cursor_result = terminal.show_cursor().context("show terminal cursor");

    combine_restore_results([
        ("terminal raw mode restore", raw_mode_result),
        ("terminal screen restore", screen_result),
        ("terminal cursor restore", cursor_result),
    ])
}

fn restore_stdout() -> Result<()> {
    let raw_mode_result = disable_raw_mode().context("disable terminal raw mode");
    let mut stdout = io::stdout();
    let screen_result = execute!(
        stdout,
        Print(DISABLE_ALTERNATE_SCROLL),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )
    .context("restore terminal screen");

    combine_restore_results([
        ("terminal raw mode restore", raw_mode_result),
        ("terminal screen restore", screen_result),
    ])
}

fn combine_restore_results(
    results: impl IntoIterator<Item = (&'static str, Result<()>)>,
) -> Result<()> {
    let mut error: Option<anyhow::Error> = None;
    for (action, result) in results {
        if let Err(next_error) = result {
            error = Some(match error {
                Some(error) => error.context(format!("{action} also failed: {next_error}")),
                None => next_error,
            });
        }
    }

    error.map_or(Ok(()), Err)
}
