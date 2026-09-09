mod app;
mod event;
mod format;
mod http;
mod logger;
mod model;
mod storage;
mod ui;

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{
        self as term_event, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste,
        EnableMouseCapture, Event, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;

use app::App;
use event::Action;
use model::ResponseData;

enum AppEvent {
    Key(crossterm::event::KeyEvent),
    Paste(String),
    Response(Result<ResponseData, String>),
}

#[tokio::main]
async fn main() -> Result<()> {
    let workspace = storage::load()?;
    let mut app = App::new(workspace);
    app.refresh_response_view();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    // Ask the terminal to disambiguate modified keys (e.g. Ctrl+Enter vs Enter),
    // if it supports the Kitty keyboard protocol. Many terminals (Terminal.app,
    // default iTerm2) don't, so this is best-effort — F5 is the reliable fallback.
    let keyboard_enhancement = supports_keyboard_enhancement().unwrap_or(false);
    if keyboard_enhancement {
        execute!(
            stdout,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    // Blocking input reader thread.
    let input_tx = tx.clone();
    std::thread::spawn(move || loop {
        if let Ok(true) = term_event::poll(Duration::from_millis(100)) {
            match term_event::read() {
                Ok(Event::Key(key)) => {
                    if key.kind == crossterm::event::KeyEventKind::Press {
                        if input_tx.send(AppEvent::Key(key)).is_err() {
                            break;
                        }
                    }
                }
                Ok(Event::Paste(text)) => {
                    if input_tx.send(AppEvent::Paste(text)).is_err() {
                        break;
                    }
                }
                _ => {}
            }
        }
    });

    let result = run(&mut terminal, &mut app, &tx, &mut rx).await;

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

    result
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    tx: &mpsc::UnboundedSender<AppEvent>,
    rx: &mut mpsc::UnboundedReceiver<AppEvent>,
) -> Result<()> {
    terminal.draw(|f| ui::draw(f, app))?;

    while let Some(evt) = rx.recv().await {
        match evt {
            AppEvent::Key(key) => match event::handle_key(app, key) {
                Action::Quit => break,
                Action::Send => trigger_send(app, tx.clone()),
                Action::None => {}
            },
            AppEvent::Paste(text) => event::handle_paste(app, text),
            AppEvent::Response(result) => {
                app.loading = false;
                match result {
                    Ok(resp) => {
                        app.push_history(Some(resp.status));
                        app.response = Some(resp);
                        app.last_error = None;
                        app.status_message = "Request completed".into();
                    }
                    Err(e) => {
                        app.push_history(None);
                        app.response = None;
                        logger::log("request failed", &e);
                        app.status_message = format!(
                            "Request failed — see Response pane (also logged to {})",
                            logger::path_hint()
                        );
                        app.last_error = Some(e);
                    }
                }
                app.refresh_response_view();
            }
        }
        if app.should_quit {
            break;
        }
        terminal.draw(|f| ui::draw(f, app))?;
    }
    Ok(())
}

fn trigger_send(app: &mut App, tx: mpsc::UnboundedSender<AppEvent>) {
    app.sync_current_from_editors();
    if app.current.url.trim().is_empty() {
        app.status_message = "URL is empty".into();
        return;
    }
    app.loading = true;
    app.status_message = "Sending request...".into();
    let req = app.current.clone();
    let env = app.active_environment().cloned();
    tokio::spawn(async move {
        // `{:#}` renders the full anyhow cause chain ("top: caused by: root
        // cause"), not just the outermost message.
        let result = http::send_request(&req, env.as_ref())
            .await
            .map_err(|e| format!("{e:#}"));
        let _ = tx.send(AppEvent::Response(result));
    });
}
