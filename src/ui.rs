use edtui::{EditorMode, EditorState, EditorTheme, EditorView, SyntaxHighlighter};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, Focus, HeaderFormField, PanelMode, Popup, ScriptTab, SidebarEntry};
use crate::model::BodyMode;

fn panel_color(app: &App, focus: Focus) -> Option<Color> {
    if app.focus != focus {
        return None;
    }
    Some(match app.panel_mode {
        PanelMode::Navigate => Color::Blue,
        PanelMode::Interact => Color::Green,
    })
}

fn focus_style(app: &App, focus: Focus) -> Style {
    match panel_color(app, focus) {
        Some(color) => Style::default().fg(color).add_modifier(Modifier::BOLD),
        None => Style::default(),
    }
}

fn border_style(app: &App, focus: Focus) -> Style {
    match panel_color(app, focus) {
        Some(color) => Style::default().fg(color),
        None => Style::default().fg(Color::DarkGray),
    }
}

fn editor_theme<'a>(block: Block<'a>) -> EditorTheme<'a> {
    EditorTheme::default()
        .base(Style::default())
        .block(block)
        .hide_status_line()
}

fn mode_label(mode: EditorMode) -> &'static str {
    match mode {
        EditorMode::Normal => "NORMAL",
        EditorMode::Insert => "INSERT",
        EditorMode::Visual => "VISUAL",
        EditorMode::Search => "SEARCH",
    }
}

fn vim_editor_theme<'a>(block: Block<'a>) -> EditorTheme<'a> {
    EditorTheme::default().base(Style::default()).block(block)
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(f.area());

    if app.maximized {
        draw_maximized(f, app, root[0]);
    } else {
        let path_params_height = path_params_panel_height(app);

        let main = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(path_params_height),
                Constraint::Min(0),
            ])
            .split(root[0]);

        draw_request_top(f, app, main[0]);
        draw_path_params(f, app, main[1]);

        let bottom = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(main[2]);

        draw_headers(f, app, bottom[0]);
        draw_body_and_response(f, app, bottom[1]);
    }

    draw_status(f, app, root[1]);

    if matches!(app.popup, Popup::CollectionFinder { .. }) {
        draw_collection_finder_popup(f, app);
        return;
    }

    match &mut app.popup {
        Popup::None => {}
        Popup::TextInput { title, state, .. } => draw_text_input_popup(f, title, state),
        Popup::HeaderForm { key, value, field, interacting, .. } => {
            draw_header_form_popup(f, key, value, *field, *interacting)
        }
        Popup::EnvManager => draw_env_manager_popup(f, app.workspace.environments.as_slice(), app.workspace.active_environment, app.env_index),
        Popup::EnvVarEditor { env_index } => {
            let env_index = *env_index;
            draw_env_var_popup(f, app.workspace.environments.as_slice(), env_index, app.env_var_index)
        }
        Popup::PathParamForm { name, value, .. } => draw_path_param_form_popup(f, name, value),
        Popup::CollectionFinder { .. } => unreachable!("handled above"),
    }
}

fn path_params_panel_height(app: &App) -> u16 {
    (app.current.path_params.len() as u16 + 2).clamp(3, 6)
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

fn draw_collection_finder_popup(f: &mut Frame, app: &mut App) {
    let (selected, query_text) = match &app.popup {
        Popup::CollectionFinder { query, selected } => (*selected, query.lines.to_string()),
        _ => return,
    };
    let matches = app.collection_finder_matches(&query_text);
    let active_env_name = app.active_environment().map(|e| e.name.clone());

    let area = centered_rect(90, 24, f.area());
    f.render_widget(Clear, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    if let Popup::CollectionFinder { query, .. } = &mut app.popup {
        let theme = editor_theme(Block::default().borders(Borders::ALL).title(
            " Find (type to filter — ^N/^P/↑↓: move, Enter: open/expand, ^R: new request, ^A: new collection, ^D: delete, Esc: close) ",
        ));
        f.render_widget(EditorView::new(query).theme(theme).single_line(true), rows[0]);
    }

    let items: Vec<ListItem> = matches
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

    let mut list_state = ratatui::widgets::ListState::default();
    if !matches.is_empty() {
        list_state.select(Some(selected));
    }

    let title = match active_env_name {
        Some(name) => format!(" Collections [env: {name}] "),
        None => " Collections ".to_string(),
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));

    f.render_stateful_widget(list, rows[1], &mut list_state);
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
                .title(" Headers (n: new, i: edit, d: delete, x: toggle) "),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    f.render_stateful_widget(list, area, &mut state);
}

fn draw_body_and_response(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    draw_body_pane(f, app, cols[0]);
    draw_scripts_pane(f, app, cols[1]);
    draw_response(f, app, cols[2]);
}

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
            let theme = vim_editor_theme(
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
                .title(" Form Data (n: new, i: edit, d: delete, x: toggle, t: text/file) "),
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

fn draw_scripts_pane(f: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(6)])
        .split(area);
    draw_script_tab_selector(f, app, rows[0]);
    draw_script_editor(f, app, rows[1]);
    draw_script_console(f, app, rows[2]);
}

fn draw_script_tab_selector(f: &mut Frame, app: &App, area: Rect) {
    let label = match app.script_tab {
        ScriptTab::Pre => "Pre-request",
        ScriptTab::Post => "Post-response",
    };
    let selector = Paragraph::new(label)
        .alignment(Alignment::Center)
        .style(focus_style(app, Focus::ScriptTab))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style(app, Focus::ScriptTab))
                .title(" Script (h/l cycle) "),
        );
    f.render_widget(selector, area);
}

fn draw_script_editor(f: &mut Frame, app: &mut App, area: Rect) {
    let title = match app.script_tab {
        ScriptTab::Pre => " Pre-request (Lua — pm.request, pm.environment, pm.console.log) ",
        ScriptTab::Post => " Post-response (Lua — pm.response, pm.environment, pm.test, pm.console.log) ",
    };
    let theme = vim_editor_theme(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style(app, Focus::ScriptEditor))
            .title(title),
    );
    let state = match app.script_tab {
        ScriptTab::Pre => &mut app.pre_script_state,
        ScriptTab::Post => &mut app.post_script_state,
    };
    let mut view = EditorView::new(state).theme(theme);
    if let Ok(highlighter) = SyntaxHighlighter::new("dracula", "lua") {
        view = view.syntax_highlighter(Some(highlighter));
    }
    f.render_widget(view, area);
}

fn draw_script_console(f: &mut Frame, app: &App, area: Rect) {
    let text = if app.script_console.is_empty() {
        "No script output yet.".to_string()
    } else {
        let tail_start = app.script_console.len().saturating_sub(4);
        app.script_console[tail_start..].join("\n")
    };
    let paragraph = Paragraph::new(text)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL).title(" Console "));
    f.render_widget(paragraph, area);
}

fn draw_maximized(f: &mut Frame, app: &mut App, area: Rect) {
    match app.focus {
        Focus::Method | Focus::Url => draw_request_top(f, app, area),
        Focus::PathParams => draw_path_params(f, app, area),
        Focus::Headers => draw_headers(f, app, area),
        Focus::BodyMode => draw_body_mode_selector(f, app, area),
        Focus::Body => draw_body_pane(f, app, area),
        Focus::ScriptTab => draw_script_tab_selector(f, app, area),
        Focus::ScriptEditor => draw_scripts_pane(f, app, area),
        Focus::Response => draw_response(f, app, area),
    }
}

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
    } else {
        panel_color(app, Focus::Response).unwrap_or(Color::DarkGray)
    };

    let theme = vim_editor_theme(
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
    let area = centered_rect(60, 5, f.area());
    f.render_widget(Clear, area);
    let theme = editor_theme(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(
                " {title} [{}] (Enter: confirm, Esc: normal/cancel) ",
                mode_label(state.mode)
            )),
    );
    f.render_widget(EditorView::new(state).theme(theme).single_line(true), area);
}

fn draw_header_form_popup(
    f: &mut Frame,
    key: &mut EditorState,
    value: &mut EditorState,
    field: HeaderFormField,
    interacting: bool,
) {
    let height = (f.area().height as u32 * 9 / 10) as u16;
    let width = (f.area().width as u32 * 9 / 10) as u16;
    let area = centered_rect(width, height, f.area());
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default().borders(Borders::ALL).title(if interacting {
            " Key / Value — editing (Esc: back to navigate, Ctrl+S: save) "
        } else {
            " Key / Value (hjkl: select, Enter/i: edit, Ctrl+S: save, Esc: cancel) "
        }),
        area,
    );
    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let border_for = |selected: bool| {
        if !selected {
            Style::default().fg(Color::DarkGray)
        } else if interacting {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Blue)
        }
    };

    let key_theme = editor_theme(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_for(field == HeaderFormField::Key))
            .title(format!(" Key [{}] ", mode_label(key.mode))),
    );
    f.render_widget(EditorView::new(key).theme(key_theme).single_line(true), inner[0]);

    let value_theme = editor_theme(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_for(field == HeaderFormField::Value))
            .title(format!(" Value [{}] ", mode_label(value.mode))),
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
