//! Open a file in `$PAGER` (default `less`) from inside the TUI: hand the terminal
//! over, wait for the pager to exit, then take it back.
use std::io::Stdout;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

/// Run the pager on `path` and return a status line. `input_paused` stops the input
/// thread from reading keys meant for the pager; `keyboard_enhancement` mirrors what
/// main() pushed at startup.
pub fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    input_paused: &AtomicBool,
    keyboard_enhancement: bool,
    path: &Path,
) -> Result<String> {
    input_paused.store(true, Ordering::SeqCst);
    // Let a poll that's already waiting (up to 100 ms) finish before the pager starts.
    std::thread::sleep(Duration::from_millis(150));

    disable_raw_mode()?;
    if keyboard_enhancement {
        execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags)?;
    }
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )?;
    terminal.show_cursor()?;

    let status = pager_command(path).status();

    enable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    if keyboard_enhancement {
        execute!(
            terminal.backend_mut(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }
    terminal.clear()?;
    input_paused.store(false, Ordering::SeqCst);
    let _ = std::fs::remove_file(path);

    Ok(match status {
        Ok(s) if s.success() => "Closed the pager".to_string(),
        Ok(s) => format!("Pager exited with {s} (set $PAGER to change it)"),
        Err(e) => format!("Couldn't start the pager: {e} (set $PAGER, e.g. less)"),
    })
}

/// `$PAGER` may carry arguments ("less -R"), so run it through the shell with the
/// file passed as a positional argument (never interpolated into the command).
fn pager_command(path: &Path) -> std::process::Command {
    let pager = std::env::var("PAGER")
        .ok()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "more".into()
            } else {
                "less".into()
            }
        });
    if cfg!(windows) {
        let mut cmd = std::process::Command::new("cmd");
        cmd.arg("/C").arg(format!("{pager} \"{}\"", path.display()));
        cmd
    } else {
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c")
            .arg(format!("{pager} \"$1\""))
            .arg("sh")
            .arg(path);
        cmd
    }
}
