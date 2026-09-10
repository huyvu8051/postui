use edtui::{EditorState, EditorTheme, EditorView, SyntaxHighlighter};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, Focus, HeaderFormField, Popup, SidebarEntry};
use crate::model::BodyMode;

fn focus_style(app: &App, focus: Focus) -> Style {
    if app.focus == focus {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn border_style(app: &App, focus: Focus) -> Style {
    if app.focus == focus {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

/// A themed block for an edtui pane. Two overrides from `EditorTheme`'s
/// defaults:
/// - `base`: edtui defaults to a hardcoded black background/white text,
///   which would paint a solid black box regardless of the terminal's own
///   theme. We use a transparent style instead, matching every other widget
///   in the app (which all inherit the terminal's background).
/// - our own status bar already carries key hints, so every edtui view
///   hides its own built-in status line to stay compact.
fn editor_theme<'a>(block: Block<'a>) -> EditorTheme<'a> {
    EditorTheme::default()
        .base(Style::default())
        .block(block)
        .hide_status_line()
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(f.area());

    if app.maximized {
        draw_maximized(f, app, root[0]);
    } else {
        let main = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(28), Constraint::Percentage(72)])
            .split(root[0]);

        draw_sidebar(f, app, main[0]);

        // Path params panel height grows with the param count (min 3 for the
        // "none" message, capped so it can't crowd out headers/body).
        let path_params_height = (app.current.path_params.len() as u16 + 2).clamp(3, 6);

        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(path_params_height),
                Constraint::Min(0),
            ])
            .split(main[1]);

        draw_request_top(f, app, right[0]);
        draw_path_params(f, app, right[1]);

        let bottom = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(right[2]);

        draw_headers(f, app, bottom[0]);
        draw_body_and_response(f, app, bottom[1]);
    }

    draw_status(f, app, root[1]);

    match &mut app.popup {
        Popup::None => {}
        Popup::TextInput { title, state, .. } => draw_text_input_popup(f, title, state),
        Popup::HeaderForm { key, value, field, .. } => draw_header_form_popup(f, key, value, *field),
        Popup::EnvManager => draw_env_manager_popup(f, app.workspace.environments.as_slice(), app.workspace.active_environment, app.env_index),
        Popup::EnvVarEditor { env_index } => {
            let env_index = *env_index;
            draw_env_var_popup(f, app.workspace.environments.as_slice(), env_index, app.env_var_index)
        }
        Popup::PathParamForm { name, value, .. } => draw_path_param_form_popup(f, name, value),
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

fn draw_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let entries = app.flatten_sidebar();
    let items: Vec<ListItem> = entries
        .iter()
        .map(|e| match e {
            SidebarEntry::Collection(ci) => {
                let c = &app.workspace.collections[*ci];
                let marker = if app.expanded.contains(&c.id) { "▾" } else { "▸" };
                ListItem::new(format!("{marker} {}", c.name))
                    .style(Style::default().add_modifier(Modifier::BOLD))
            }
            SidebarEntry::Request(ci, ri) => {
                let r = &app.workspace.collections[*ci].requests[*ri];
                ListItem::new(format!("    {} {}", r.method.as_str(), r.name))
            }
        })
        .collect();

    app.sidebar_state.select(if entries.is_empty() {
        None
    } else {
        Some(app.sidebar_index)
    });

    let title = if let Some(env) = app.active_environment() {
        format!(" Collections [env: {}] ", env.name)
    } else {
        " Collections ".to_string()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style(app, Focus::Sidebar))
                .title(title),
        )
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));

    f.render_stateful_widget(list, area, &mut app.sidebar_state);
}

fn draw_request_top(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(12), Constraint::Min(0)])
        .split(area);

    let method = Paragraph::new(app.current.method.as_str())
        .alignment(Alignment::Center)
        .style(focus_style(app, Focus::Method))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style(app, Focus::Method))
                .title(" Method "),
        );
    f.render_widget(method, cols[0]);

    let theme = editor_theme(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style(app, Focus::Url))
            .title(" URL (vim keys — i insert, hjkl move; {{var}} for env vars, :param below) "),
    );
    f.render_widget(
        EditorView::new(&mut app.url_state).theme(theme).single_line(true),
        cols[1],
    );
}

fn draw_path_params(f: &mut Frame, app: &mut App, area: Rect) {
    if app.current.path_params.is_empty() {
        let placeholder = Paragraph::new("No path params — add a :name segment to the URL (e.g. /:tenantId/...)")
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style(app, Focus::PathParams))
                    .title(" Path Params "),
            );
        f.render_widget(placeholder, area);
        return;
    }

    let items: Vec<ListItem> = app
        .current
        .path_params
        .iter()
        .map(|(k, v)| {
            let style = if v.is_empty() {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            let shown = if v.is_empty() { "<empty>" } else { v.as_str() };
            ListItem::new(format!(":{k} = {shown}")).style(style)
        })
        .collect();

    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(app.path_param_index));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style(app, Focus::PathParams))
                .title(" Path Params (i/Enter: edit value) "),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    f.render_stateful_widget(list, area, &mut state);
}

fn draw_headers(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .current
        .headers
        .iter()
        .map(|h| {
            let enabled = if h.enabled { "[x]" } else { "[ ]" };
            let style = if h.enabled {
                Style::default()
            } else {
                Style::default().fg(Color::DarkGray)
            };
            ListItem::new(format!("{enabled} {}: {}", h.key, h.value)).style(style)
        })
        .collect();

    let mut state = ratatui::widgets::ListState::default();
    if !app.current.headers.is_empty() {
        state.select(Some(app.header_index));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style(app, Focus::Headers))
                .title(" Headers (n: new, i: edit, d: delete, space: toggle) "),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    f.render_stateful_widget(list, area, &mut state);
}

fn draw_body_and_response(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_body_pane(f, app, cols[0]);
    draw_response(f, app, cols[1]);
}

/// The Body pane: a Raw/Form Data/Binary selector strip, then whatever that
/// mode needs (see `draw_body_content`).
fn draw_body_pane(f: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);
    draw_body_mode_selector(f, app, rows[0]);
    draw_body_content(f, app, rows[1]);
}

fn draw_body_mode_selector(f: &mut Frame, app: &App, area: Rect) {
    let selector = Paragraph::new(app.current.body_mode.as_str())
        .alignment(Alignment::Center)
        .style(focus_style(app, Focus::BodyMode))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style(app, Focus::BodyMode))
                .title(" Body Type (h/l cycle) "),
        );
    f.render_widget(selector, area);
}

fn draw_body_content(f: &mut Frame, app: &mut App, area: Rect) {
    match app.current.body_mode {
        BodyMode::Raw => {
            let theme = editor_theme(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style(app, Focus::Body))
                    .title(" Body (vim keys — i insert, hjkl move, f: format JSON) "),
            );
            f.render_widget(EditorView::new(&mut app.body_state).theme(theme), area);
        }
        BodyMode::FormData => draw_form_data(f, app, area),
        BodyMode::Binary => draw_binary(f, app, area),
    }
}

fn draw_form_data(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .current
        .form_fields
        .iter()
        .map(|field| {
            let enabled = if field.enabled { "[x]" } else { "[ ]" };
            let kind = if field.value.is_file() { "File" } else { "Text" };
            let style = if field.enabled {
                Style::default()
            } else {
                Style::default().fg(Color::DarkGray)
            };
            ListItem::new(format!(
                "{enabled} {}: {} <{kind}>",
                field.key,
                field.value.as_str()
            ))
            .style(style)
        })
        .collect();

    let mut state = ratatui::widgets::ListState::default();
    if !app.current.form_fields.is_empty() {
        state.select(Some(app.form_field_index));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style(app, Focus::Body))
                .title(" Form Data (n: new, i: edit, d: delete, space: toggle, t: text/file) "),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    f.render_stateful_widget(list, area, &mut state);
}

fn draw_binary(f: &mut Frame, app: &App, area: Rect) {
    let (text, style) = if app.current.binary_file.is_empty() {
        (
            "No file selected — press i or Enter to set a path".to_string(),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        let exists = std::path::Path::new(&app.current.binary_file).is_file();
        let marker = if exists { "✓" } else { "✗ not found" };
        let color = if exists { Color::Green } else { Color::Red };
        (
            format!("{marker}  {}", app.current.binary_file),
            Style::default().fg(color),
        )
    };

    let paragraph = Paragraph::new(text).style(style).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style(app, Focus::Body))
            .title(" Binary file (i/Enter: set path, d: clear) "),
    );
    f.render_widget(paragraph, area);
}

/// Render whichever pane is focused across the whole main area (`F3`),
/// reusing each pane's normal single-panel renderer. `Method` piggybacks on
/// `draw_request_top` (it shares a row with URL) since maximizing the tiny
/// Method box on its own isn't useful.
fn draw_maximized(f: &mut Frame, app: &mut App, area: Rect) {
    match app.focus {
        Focus::Sidebar => draw_sidebar(f, app, area),
        Focus::Method | Focus::Url => draw_request_top(f, app, area),
        Focus::PathParams => draw_path_params(f, app, area),
        Focus::Headers => draw_headers(f, app, area),
        Focus::BodyMode => draw_body_mode_selector(f, app, area),
        Focus::Body => draw_body_pane(f, app, area),
        Focus::Response => draw_response(f, app, area),
    }
}

/// The Response pane is a real edtui vim buffer at all times — no separate
/// "enter/exit view mode" step. `hjkl`, `gg`/`G`, `/` search + `n`/`N`, `v`
/// visual + `y` yank (system clipboard) are all native to edtui. `F4` (open
/// a link on the cursor's line) is our own addition, handled in `event.rs`.
/// Search/selection state naturally survives losing and regaining focus
/// because the buffer is only rebuilt on a new response (`refresh_response_view`),
/// never on refocus.
fn draw_response(f: &mut Frame, app: &mut App, area: Rect) {
    let title = if app.loading {
        " Response (sending...) ".to_string()
    } else if app.last_error.is_some() {
        " Response — request failed ".to_string()
    } else if let Some(r) = &app.response {
        let kind = if app.response_is_json { ", JSON" } else { "" };
        format!(
            " Response — {} ({} ms{kind}) — hjkl gg/G /search v y F4 ",
            r.status, r.duration_ms
        )
    } else {
        " Response ".to_string()
    };

    let border_color = if app.last_error.is_some() {
        Color::Red
    } else if app.focus == Focus::Response {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let theme = editor_theme(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(title),
    );
    let mut view = EditorView::new(&mut app.response_state).theme(theme);
    if app.response_is_json {
        if let Ok(highlighter) = SyntaxHighlighter::new("dracula", "json") {
            view = view.syntax_highlighter(Some(highlighter));
        }
    }
    f.render_widget(view, area);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let status = Paragraph::new(app.status_message.as_str()).style(Style::default().fg(Color::Gray));
    f.render_widget(status, area);
}

fn draw_text_input_popup(f: &mut Frame, title: &str, state: &mut EditorState) {
    let area = centered_rect(60, 3, f.area());
    f.render_widget(Clear, area);
    let theme = editor_theme(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {title} (Enter to confirm, Esc to cancel) ")),
    );
    f.render_widget(EditorView::new(state).theme(theme).single_line(true), area);
}

fn draw_header_form_popup(f: &mut Frame, key: &mut EditorState, value: &mut EditorState, field: HeaderFormField) {
    let area = centered_rect(80, 12, f.area());
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(" Key / Value (Tab to switch field, Enter to confirm) "),
        area,
    );
    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(4), Constraint::Length(4)])
        .split(area);

    let key_border = if field == HeaderFormField::Key {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let key_theme = editor_theme(
        Block::default().borders(Borders::ALL).border_style(key_border).title(" Key "),
    );
    f.render_widget(EditorView::new(key).theme(key_theme).single_line(true), inner[0]);

    let value_border = if field == HeaderFormField::Value {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let value_theme = editor_theme(
        Block::default()
            .borders(Borders::ALL)
            .border_style(value_border)
            .title(" Value "),
    );
    f.render_widget(EditorView::new(value).theme(value_theme).single_line(true), inner[1]);
}

fn draw_path_param_form_popup(f: &mut Frame, name: &str, value: &mut EditorState) {
    let area = centered_rect(60, 3, f.area());
    f.render_widget(Clear, area);
    let theme = editor_theme(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!(" :{name} (Enter to confirm, Esc to cancel) ")),
    );
    f.render_widget(EditorView::new(value).theme(theme).single_line(true), area);
}

fn draw_env_manager_popup(
    f: &mut Frame,
    environments: &[crate::model::Environment],
    active: Option<uuid::Uuid>,
    env_index: usize,
) {
    let area = centered_rect(60, 12, f.area());
    f.render_widget(Clear, area);

    let items: Vec<ListItem> = environments
        .iter()
        .map(|e| {
            let marker = if active == Some(e.id) { "●" } else { " " };
            ListItem::new(format!("{marker} {}", e.name))
        })
        .collect();

    let mut state = ratatui::widgets::ListState::default();
    if !environments.is_empty() {
        state.select(Some(env_index));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Environments (n: new, Enter: activate, v: variables, d: delete, Esc: close) "),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    f.render_stateful_widget(list, area, &mut state);
}

fn draw_env_var_popup(f: &mut Frame, environments: &[crate::model::Environment], env_index: usize, var_index: usize) {
    let area = centered_rect(60, 12, f.area());
    f.render_widget(Clear, area);

    let Some(env) = environments.get(env_index) else {
        return;
    };

    let items: Vec<ListItem> = env
        .variables
        .iter()
        .map(|(k, v)| ListItem::new(format!("{k} = {v}")))
        .collect();

    let mut state = ratatui::widgets::ListState::default();
    if !env.variables.is_empty() {
        state.select(Some(var_index));
    }

    let list = List::new(items)
        .block(
            Block::default().borders(Borders::ALL).title(format!(
                " Variables — {} (n: new, i: edit, d: delete, Esc: back) ",
                env.name
            )),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    f.render_stateful_widget(list, area, &mut state);
}
