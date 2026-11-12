mod app;
mod completion;
mod event;
mod format;
mod http;
mod logger;
mod model;
mod script;
mod session;
mod storage;
#[cfg(test)]
mod test_support;
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
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
        supports_keyboard_enhancement,
    },
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;

use app::App;
use event::Action;
use model::ResponseData;

// Events are moved through a channel one at a time; the size difference doesn't matter.
#[allow(clippy::large_enum_variant)]
enum AppEvent {
    Key(crossterm::event::KeyEvent),
    Paste(String),
    /// A finished send, tagged with the request as it was when sent.
    Response {
        sent: model::RequestItem,
        result: Result<ResponseData, String>,
    },
    /// Idle heartbeat from the input thread (~10/s), used for debounced autosave.
    Tick,
    /// The terminal was resized; redraw at the new size.
    Resize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let workspace = storage::load()?;
    let mut app = App::new(workspace);
    app.prune_tabs();
    app.history = storage::load_history();
    app.load_response_history(storage::read_response_lines());
    app.restore_session(storage::load_session());
    app.refresh_response_view();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
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

    let input_tx = tx.clone();
    std::thread::spawn(move || {
        loop {
            let app_event = match term_event::poll(Duration::from_millis(100)) {
                Ok(false) => Some(AppEvent::Tick),
                Ok(true) => match term_event::read() {
                    Ok(Event::Key(key)) if key.kind == crossterm::event::KeyEventKind::Press => {
                        Some(AppEvent::Key(key))
                    }
                    Ok(Event::Paste(text)) => Some(AppEvent::Paste(text)),
                    Ok(Event::Resize(_, _)) => Some(AppEvent::Resize),
                    _ => None,
                },
                Err(_) => None,
            };
            if let Some(app_event) = app_event
                && input_tx.send(app_event).is_err()
            {
                break; // the app has quit
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
        if let AppEvent::Tick = evt {
            if app.session_save_due() {
                app.save_session();
                terminal.draw(|f| ui::draw(f, app))?;
            }
            continue;
        }
        if let AppEvent::Resize = evt {
            // draw() picks up the new size and repaints everything.
            terminal.draw(|f| ui::draw(f, app))?;
            continue;
        }
        app.mark_session_changed();
        match evt {
            AppEvent::Tick | AppEvent::Resize => {}
            AppEvent::Key(key) => match event::handle_key(app, key) {
                Action::Quit => {
                    app.save_session();
                    break;
                }
                Action::Send => trigger_send(app, tx.clone()),
                Action::None => {}
            },
            AppEvent::Paste(text) => event::handle_paste(app, text),
            AppEvent::Response { sent, result } => {
                let (response, error) = match result {
                    Ok(resp) => (Some(resp), None),
                    Err(e) => (None, Some(e)),
                };
                app.push_history(
                    &sent,
                    response.as_ref().map(|r| r.status),
                    response.as_ref().map(|r| r.duration_ms),
                );
                match (&response, &error) {
                    (Some(resp), _) => {
                        app.status_message = "Request completed".into();
                        if !sent.post_response_script.trim().is_empty() {
                            let mut env = app.active_environment().cloned();
                            let outcome = script::run_post_response(
                                &sent.post_response_script,
                                resp,
                                &mut env,
                            );
                            app.push_script_log(outcome.logs);
                            app.replace_environment_matching_id(env);
                            if let Some(err) = outcome.error {
                                app.status_message = format!(
                                    "Request completed, but post-response script failed: {err}"
                                );
                            }
                        }
                    }
                    (None, Some(e)) => {
                        logger::log("request failed", e);
                        app.status_message = format!(
                            "Request failed — see Response pane (also logged to {})",
                            logger::path_hint()
                        );
                    }
                    (None, None) => {}
                }
                app.deliver_response(&sent, response, error);
            }
        }
        if app.should_quit {
            app.save_session();
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

    let sent = app.current.clone();
    let mut req = app.current.clone();
    let mut env = app.active_environment().cloned();
    if !req.pre_request_script.trim().is_empty() {
        let pre_request_script = req.pre_request_script.clone();
        let outcome = script::run_pre_request(&pre_request_script, &mut req, &mut env);
        app.push_script_log(outcome.logs);
        app.replace_environment_matching_id(env.clone());
        if let Some(err) = outcome.error {
            let msg = format!("Pre-request script failed: {err}");
            app.status_message = msg.clone();
            app.last_error = Some(msg);
            return;
        }
    }

    app.begin_send(sent.id);
    app.status_message = format!("Sending '{}'...", sent.name);
    tokio::spawn(async move {
        let result = http::send_request(&req, env.as_ref())
            .await
            .map_err(full_cause_chain);
        let _ = tx.send(AppEvent::Response { sent, result });
    });
}

fn full_cause_chain(e: anyhow::Error) -> String {
    format!("{e:#}")
}
