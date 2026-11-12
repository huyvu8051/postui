use edtui::{EditorMode, EditorState, EditorTheme, EditorView, SyntaxHighlighter};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
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
        let mut content = root[0];
        if app.sidebar_visible {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(32), Constraint::Min(0)])
                .split(root[0]);
            draw_tabs_sidebar(f, app, cols[0]);
            content = cols[1];
        }
        let path_params_height = path_params_panel_height(app);

        let main = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(path_params_height),
                Constraint::Min(0),
            ])
            .split(content);

        draw_request_top(f, app, main[0]);
        draw_path_params(f, app, main[1]);

        let bottom = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(main[2]);

        draw_headers_and_scripts(f, app, bottom[0]);
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
        Popup::History { selected } => {
            let selected = *selected;
            draw_history_popup(f, app, selected)
        }
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
    let (selected, query_text, browse, confirm_delete) = match &app.popup {
        Popup::CollectionFinder { query, selected, browse, confirm_delete } => {
            (*selected, query.lines.to_string(), *browse, *confirm_delete)
        }
        _ => return,
    };
    let matches = app.collection_finder_matches(&query_text, browse);
    let recent_mode = !browse && query_text.trim().is_empty() && !app.recent_entries().is_empty();
    let active_env_name = app.active_environment().map(|e| e.name.clone());

    let height = (f.area().height as u32 * 9 / 10) as u16;
    let width = (f.area().width as u32 * 9 / 10) as u16;
    let area = centered_rect(width, height, f.area());
    f.render_widget(Clear, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    if let Popup::CollectionFinder { query, .. } = &mut app.popup {
        let (title, border) = if browse {
            (" Search — / or i: type ", Color::DarkGray)
        } else {
            (" Search — type to filter, Enter: open, Esc: browse list ", Color::Cyan)
        };
        let theme = editor_theme(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border))
                .title(title),
        );
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
                let c = &app.workspace.collections[*ci];
                let r = &c.requests[*ri];
                let star = if app.is_dirty(r.id) { "*" } else { " " };
                if recent_mode {
                    ListItem::new(format!("{star} {} / {} {}", c.name, r.method.as_str(), r.name))
                } else {
                    ListItem::new(format!("  {star} {} {}", r.method.as_str(), r.name))
                }
            }
        })
        .collect();

    let mut list_state = ratatui::widgets::ListState::default();
    if !matches.is_empty() {
        list_state.select(Some(selected));
    }

    let heading = if recent_mode { "Recent" } else { "Collections" };
    let title = match active_env_name {
        Some(name) => format!(" {heading} [env: {name}] "),
        None => format!(" {heading} "),
    };

    let list_border = if browse { Color::Cyan } else { Color::DarkGray };
    let list_hint = if browse {
        "j/k: move, h/l: collapse/expand, Enter: open, a: new request, A: new collection, d: delete, /: search, Esc: close"
    } else {
        "^N/^P: move, ^R: new request, ^A: new collection, ^D: delete"
    };
    let bottom = match confirm_delete {
        Some(SidebarEntry::Collection(ci)) => {
            let c = &app.workspace.collections[ci];
            Line::from(format!(
                " Delete collection \"{}\" and its {} request(s)? y/n ",
                c.name,
                c.requests.len()
            ))
            .style(Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD))
        }
        Some(SidebarEntry::Request(ci, ri)) => {
            let r = &app.workspace.collections[ci].requests[ri];
            Line::from(format!(" Delete request \"{}\"? y/n ", r.name))
                .style(Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD))
        }
        None => Line::from(format!(" {list_hint} ")),
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(list_border))
                .title(title)
                .title_bottom(bottom),
        )
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

fn draw_headers_and_scripts(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_headers(f, app, cols[0]);
    draw_scripts_pane(f, app, cols[1]);
}

fn draw_body_and_response(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_body_pane(f, app, cols[0]);
    draw_response(f, app, cols[1]);
}

/// "Raw  [Form Data]  Binary" — the active body type is bracketed.
fn body_type_tabs(active: BodyMode) -> String {
    BodyMode::ALL
        .iter()
        .map(|m| if *m == active { format!("[{}]", m.as_str()) } else { m.as_str().to_string() })
        .collect::<Vec<_>>()
        .join(" ")
}

fn draw_body_pane(f: &mut Frame, app: &mut App, area: Rect) {
    draw_body_content(f, app, area);
}

fn draw_body_content(f: &mut Frame, app: &mut App, area: Rect) {
    match app.current.body_mode {
        BodyMode::Raw => {
            let theme = vim_editor_theme(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style(app, Focus::Body))
                    .title(format!(" Body {} — [ ]: type, i: insert, f: format JSON ", body_type_tabs(app.current.body_mode))),
            );
            let is_json = body_looks_like_json(app);
            let mut view = EditorView::new(&mut app.body_state).theme(theme);
            if is_json {
                if let Ok(highlighter) = SyntaxHighlighter::new("dracula", "json") {
                    view = view.syntax_highlighter(Some(highlighter));
                }
            }
            f.render_widget(view, area);
        }
        BodyMode::FormData => draw_form_data(f, app, area),
        BodyMode::Binary => draw_binary(f, app, area),
    }
}

/// Highlight the raw body as JSON when a Content-Type header says so, or the text
/// starts with `{`/`[`. Deliberately not "parses as JSON": half-typed JSON must stay
/// highlighted while editing.
fn body_looks_like_json(app: &App) -> bool {
    let by_header = app.current.headers.iter().any(|h| {
        h.enabled
            && h.key.eq_ignore_ascii_case("content-type")
            && h.value.to_ascii_lowercase().contains("json")
    });
    if by_header {
        return true;
    }
    let text = app.body_state.lines.to_string();
    matches!(text.trim_start().chars().next(), Some('{' | '['))
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
                .title(format!(" Body {} — [ ]: type, n: new, i: edit, d: delete, x: toggle, t: text/file ", body_type_tabs(app.current.body_mode))),
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
            .title(format!(" Body {} — [ ]: type, i/Enter: set path, d: clear ", body_type_tabs(app.current.body_mode))),
    );
    f.render_widget(paragraph, area);
}

fn draw_scripts_pane(f: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(6)])
        .split(area);
    draw_script_editor(f, app, rows[0]);
    draw_script_console(f, app, rows[1]);
}

fn draw_script_editor(f: &mut Frame, app: &mut App, area: Rect) {
    let tabs = match app.script_tab {
        ScriptTab::Pre => "[Pre-request] Post-response",
        ScriptTab::Post => "Pre-request [Post-response]",
    };
    let api = match app.script_tab {
        ScriptTab::Pre => "pm.request, pm.environment, pm.clipboard.set, pm.console.log",
        ScriptTab::Post => "pm.response, pm.environment, pm.test, pm.clipboard.set, pm.console.log",
    };
    let title = format!(" Script {tabs} — [ ]: type, i: insert (Lua: {api}) ");
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

fn draw_script_console(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style(app, Focus::Console))
        .title(" Console (read-only — hjkl gg/G /search v y) ");
    if app.script_console.is_empty() {
        let paragraph = Paragraph::new("No script output yet.")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(paragraph, area);
        return;
    }
    let theme = vim_editor_theme(block);
    f.render_widget(EditorView::new(&mut app.console_state).theme(theme), area);
}

fn draw_tabs_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let active = app.source.map(|_| app.current.id);
    let items: Vec<ListItem> = app
        .workspace
        .open_tabs
        .iter()
        .filter_map(|id| {
            let (ci, ri) = app.find_request(*id)?;
            let saved = &app.workspace.collections[ci].requests[ri];
            let (method, name) = if Some(*id) == active {
                (app.current.method, app.current.name.clone())
            } else {
                (saved.method, saved.name.clone())
            };
            let marker = if Some(*id) == active { "▶" } else { " " };
            let dirty = if app.is_dirty(*id) { "*" } else { " " };
            let style = if Some(*id) == active {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Some(ListItem::new(format!("{marker}{dirty} {:<6} {name}", method.as_str())).style(style))
        })
        .collect();

    let mut state = ratatui::widgets::ListState::default();
    if app.focus == Focus::Tabs && !items.is_empty() {
        state.select(Some(app.tab_index));
    }

    let bottom = match app.tab_confirm_close.and_then(|id| app.find_request(id)) {
        Some((ci, ri)) => Line::from(format!(
            " Discard unsaved changes to \"{}\"? y/n ",
            app.workspace.collections[ci].requests[ri].name
        ))
        .style(Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)),
        None => Line::from(" Enter: open, d: close, J/K: move "),
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style(app, Focus::Tabs))
                .title(format!(" Tabs ({}) ", app.workspace.open_tabs.len()))
                .title_bottom(bottom),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_history_popup(f: &mut Frame, app: &App, selected: usize) {
    let height = (f.area().height as u32 * 9 / 10) as u16;
    let width = (f.area().width as u32 * 9 / 10) as u16;
    let area = centered_rect(width, height, f.area());
    f.render_widget(Clear, area);

    let items: Vec<ListItem> = app
        .history
        .iter()
        .rev()
        .map(|e| {
            let (status, color) = match e.status {
                Some(s) if s < 400 => (s.to_string(), Color::Green),
                Some(s) => (s.to_string(), Color::Red),
                None => ("ERR".to_string(), Color::Red),
            };
            let ms = e.duration_ms.map(|d| format!("{d}ms")).unwrap_or_default();
            let gone = if e.request_id.and_then(|id| app.find_request(id)).is_none() { " (deleted)" } else { "" };
            let line = Line::from(vec![
                ratatui::text::Span::raw(format!("{}  ", e.timestamp.format("%m-%d %H:%M:%S"))),
                ratatui::text::Span::styled(format!("{status:>3}"), Style::default().fg(color)),
                ratatui::text::Span::raw(format!(" {ms:>7}  {:<7} {}{gone}  ", e.method.as_str(), e.name)),
                ratatui::text::Span::styled(e.url.clone(), Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut state = ratatui::widgets::ListState::default();
    if !items.is_empty() {
        state.select(Some(selected));
    }
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(format!(" History ({}) ", app.history.len()))
                .title_bottom(" j/k: move, Enter: open request, Esc: close "),
        )
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_maximized(f: &mut Frame, app: &mut App, area: Rect) {
    match app.focus {
        Focus::Tabs => draw_tabs_sidebar(f, app, area),
        Focus::Method | Focus::Url => draw_request_top(f, app, area),
        Focus::PathParams => draw_path_params(f, app, area),
        Focus::Headers => draw_headers(f, app, area),
        Focus::Body => draw_body_pane(f, app, area),
        Focus::ScriptEditor => draw_scripts_pane(f, app, area),
        Focus::Console => draw_script_console(f, app, area),
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
