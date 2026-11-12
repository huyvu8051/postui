use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use edtui::EditorMode;
use uuid::Uuid;

use crate::app::{
    single_line_state, vim_line_state, App, Direction, Focus, HeaderFormField, HeaderFormTarget, LeaderState,
    PanelMode, Popup, RequestSource, ScriptTab, SidebarEntry, TextInputTarget,
};
use crate::model::{BodyMode, Collection, Environment, FormField, FormValue, Header, RequestItem};

fn is_vim_editor_focus(app: &App) -> bool {
    matches!(app.focus, Focus::Url | Focus::Response | Focus::ScriptEditor | Focus::Console)
        || (app.focus == Focus::Body && app.current.body_mode == BodyMode::Raw)
}

pub enum Action {
    None,
    Send,
    Quit,
}

pub fn handle_paste(app: &mut App, text: String) {
    match &mut app.popup {
        Popup::TextInput { state, .. } => {
            app.editor_events.on_paste_event(text, state);
            return;
        }
        Popup::HeaderForm { key, value, field, interacting, .. } => {
            if !*interacting {
                return;
            }
            let state = match field {
                HeaderFormField::Key => key,
                HeaderFormField::Value => value,
            };
            app.editor_events.on_paste_event(text, state);
            return;
        }
        Popup::PathParamForm { value, .. } => {
            app.editor_events.on_paste_event(text, value);
            return;
        }
        _ => {}
    }

    if app.panel_mode != PanelMode::Interact {
        return;
    }

    match app.focus {
        Focus::Url => app.editor_events.on_paste_event(text, &mut app.url_state),
        Focus::Body if app.current.body_mode == BodyMode::Raw => {
            app.editor_events.on_paste_event(text, &mut app.body_state)
        }
        Focus::Response => app.editor_events.on_paste_event(text, &mut app.response_state),
        Focus::ScriptEditor => {
            let state = match app.script_tab {
                ScriptTab::Pre => &mut app.pre_script_state,
                ScriptTab::Post => &mut app.post_script_state,
            };
            app.editor_events.on_paste_event(text, state);
        }
        _ => {}
    }
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    if !matches!(app.popup, Popup::None) {
        handle_popup_key(app, key);
        return Action::None;
    }

    if app.tab_confirm_close.is_some() {
        handle_tab_close_confirm(app, key);
        return Action::None;
    }

    if let Some(action) = handle_always_global_key(app, key) {
        return action;
    }

    if app.panel_mode == PanelMode::Interact && is_vim_editor_focus(app) {
        return handle_editor_pane_key(app, key);
    }

    if handle_leader_key(app, key) {
        return Action::None;
    }

    if handle_tab_shortcut(app, key) {
        return Action::None;
    }

    match key.code {
        KeyCode::Char('q') => return Action::Quit,
        _ => {}
    }

    match app.panel_mode {
        PanelMode::Navigate => handle_navigate_key(app, key),
        PanelMode::Interact => handle_interact_key(app, key),
    }

    Action::None
}

fn handle_always_global_key(app: &mut App, key: KeyEvent) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Enter {
        app.sync_current_from_editors();
        return Some(Action::Send);
    }
    if key.code == KeyCode::F(5) {
        app.sync_current_from_editors();
        return Some(Action::Send);
    }
    if key.code == KeyCode::F(3) {
        toggle_maximized(app);
        return Some(Action::None);
    }
    None
}

fn toggle_maximized(app: &mut App) {
    app.maximized = !app.maximized;
    app.status_message = if app.maximized {
        "Panel maximized — space z (or F3) to restore layout".into()
    } else {
        "Layout restored".into()
    };
}

fn handle_navigate_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => app.move_focus(Direction::Left),
        KeyCode::Char('j') | KeyCode::Down => app.move_focus(Direction::Down),
        KeyCode::Char('k') | KeyCode::Up => app.move_focus(Direction::Up),
        KeyCode::Char('l') | KeyCode::Right => app.move_focus(Direction::Right),
        KeyCode::Enter => app.panel_mode = PanelMode::Interact,
        KeyCode::Char('[') if app.focus == Focus::Body => cycle_body_mode(app, false),
        KeyCode::Char(']') if app.focus == Focus::Body => cycle_body_mode(app, true),
        KeyCode::Char('[') if app.focus == Focus::ScriptEditor => app.script_tab = app.script_tab.prev(),
        KeyCode::Char(']') if app.focus == Focus::ScriptEditor => app.script_tab = app.script_tab.next(),
        KeyCode::Char('[') if app.focus == Focus::Response => app.step_response_history(true),
        KeyCode::Char(']') if app.focus == Focus::Response => app.step_response_history(false),
        KeyCode::Char('x') if app.focus == Focus::Response => app.run_post_script_on_shown_response(),
        _ => {}
    }
}

fn handle_interact_key(app: &mut App, key: KeyEvent) {
    if key.code == KeyCode::Esc {
        app.panel_mode = PanelMode::Navigate;
        return;
    }

    match app.focus {
        Focus::Tabs => handle_tabs_key(app, key),
        Focus::Method => handle_method_key(app, key),
        Focus::PathParams => handle_path_params_key(app, key),
        Focus::Headers => handle_headers_key(app, key),
        Focus::Body => handle_body_field_key(app, key),
        Focus::Url | Focus::Response | Focus::ScriptEditor | Focus::Console => unreachable!("handled above"),
    }
}

fn handle_leader_key(app: &mut App, key: KeyEvent) -> bool {
    match (app.leader, key.code) {
        (LeaderState::Idle, KeyCode::Char(' ')) => {
            app.leader = LeaderState::Awaiting;
            true
        }
        (LeaderState::Idle, _) => false,
        (LeaderState::Awaiting, code) => {
            app.leader = LeaderState::Idle;
            match code {
                KeyCode::Char('f') => open_collection_finder(app),
                KeyCode::Char('t') => toggle_tabs_sidebar(app),
                KeyCode::Char('w') => app.save_current(),
                KeyCode::Char('W') => app.save_all(),
                KeyCode::Char('H') => app.popup = Popup::History { selected: 0 },
                KeyCode::Char('z') => toggle_maximized(app),
                KeyCode::Char('e') => {
                    app.popup = Popup::EnvManager;
                    app.env_index = 0;
                }
                KeyCode::Char(c @ '1'..='9') => app.open_slot(c as u8 - b'0'),
                KeyCode::Char(c) => {
                    if let Some(target) = leader_focus_target(c) {
                        jump_into(app, target);
                    }
                }
                _ => {}
            }
            true
        }
    }
}

/// `space t`: show the sidebar and jump into it; pressed again while in it, hide it.
fn toggle_tabs_sidebar(app: &mut App) {
    if app.sidebar_visible && app.focus == Focus::Tabs {
        app.sidebar_visible = false;
        app.focus = app.focus_before_tabs;
        app.panel_mode = PanelMode::Navigate;
        return;
    }
    if app.focus != Focus::Tabs {
        app.focus_before_tabs = app.focus;
    }
    app.sidebar_visible = true;
    // Start on the loaded request's tab.
    if let Some(pos) = app.workspace.open_tabs.iter().position(|t| *t == app.current.id) {
        app.tab_index = pos;
    }
    jump_into(app, Focus::Tabs);
}

fn handle_tabs_key(app: &mut App, key: KeyEvent) {
    let len = app.workspace.open_tabs.len();
    let selected = app.workspace.open_tabs.get(app.tab_index).copied();
    if std::mem::take(&mut app.tab_mark_pending) {
        assign_slot_key(app, selected, key);
        return;
    }
    match key.code {
        KeyCode::Char('j') | KeyCode::Down if len > 0 => {
            app.tab_index = (app.tab_index + 1).min(len - 1);
        }
        KeyCode::Char('k') | KeyCode::Up => app.tab_index = app.tab_index.saturating_sub(1),
        KeyCode::Char('g') => app.tab_index = 0,
        KeyCode::Char('G') => app.tab_index = len.saturating_sub(1),
        KeyCode::Enter | KeyCode::Char('l') => {
            if let Some((ci, ri)) = selected.and_then(|id| app.find_request(id)) {
                app.load_request(ci, ri);
            }
        }
        KeyCode::Char('m') if selected.is_some() => app.tab_mark_pending = true,
        KeyCode::Char('d') | KeyCode::Char('x') => {
            if let Some(id) = selected {
                if app.is_dirty(id) {
                    app.tab_confirm_close = Some(id);
                } else {
                    app.close_tab(id);
                }
            }
        }
        _ => {}
    }
}

/// `H`/`L`: previous/next open tab (wraps). `J`/`K` while in the sidebar: move the
/// selected tab down/up. Only reached outside vim buffers.
fn handle_tab_shortcut(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('H') => cycle_tab(app, false),
        KeyCode::Char('L') => cycle_tab(app, true),
        KeyCode::Char('J') if app.focus == Focus::Tabs => move_tab(app, true),
        KeyCode::Char('K') if app.focus == Focus::Tabs => move_tab(app, false),
        _ => return false,
    }
    true
}

fn cycle_tab(app: &mut App, forward: bool) {
    let tabs = &app.workspace.open_tabs;
    let len = tabs.len();
    if len == 0 {
        app.status_message = "No open tabs".into();
        return;
    }
    let next = match tabs.iter().position(|t| app.source.is_some() && *t == app.current.id) {
        Some(pos) if forward => (pos + 1) % len,
        Some(pos) => (pos + len - 1) % len,
        None => 0,
    };
    let id = tabs[next];
    if let Some((ci, ri)) = app.find_request(id) {
        app.load_request(ci, ri);
        app.tab_index = next;
    }
}

fn move_tab(app: &mut App, down: bool) {
    let len = app.workspace.open_tabs.len();
    let i = app.tab_index;
    if i >= len {
        return;
    }
    let j = if down { i + 1 } else { i.wrapping_sub(1) };
    if j >= len {
        return;
    }
    app.workspace.open_tabs.swap(i, j);
    app.tab_index = j;
    app.persist();
}

/// The key after `m` in the tabs sidebar: 1-9 assigns the selected tab to that
/// quick slot, 0 clears its slot, anything else cancels.
fn assign_slot_key(app: &mut App, selected: Option<Uuid>, key: KeyEvent) {
    let Some(id) = selected else { return };
    match key.code {
        KeyCode::Char(c @ '1'..='9') => app.assign_slot(id, c as u8 - b'0'),
        KeyCode::Char('0') => app.clear_slot_of(id),
        _ => app.status_message = "Slot assignment cancelled".into(),
    }
}

/// A pending "discard unsaved changes?" swallows the next key: y closes, anything else cancels.
fn handle_tab_close_confirm(app: &mut App, key: KeyEvent) {
    let Some(id) = app.tab_confirm_close.take() else {
        return;
    };
    if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
        app.close_tab(id);
    } else {
        app.status_message = "Close cancelled".into();
    }
}

/// `space <key>` pane shortcuts.
fn leader_focus_target(c: char) -> Option<Focus> {
    Some(match c {
        'u' => Focus::Url,
        'm' => Focus::Method,
        'h' => Focus::Headers,
        'p' => Focus::PathParams,
        's' => Focus::ScriptEditor,
        'c' => Focus::Console,
        'b' => Focus::Body,
        'r' => Focus::Response,
        _ => return None,
    })
}

/// Focus a pane and enter it directly (vim buffers land in Normal mode).
fn jump_into(app: &mut App, target: Focus) {
    app.focus = target;
    app.panel_mode = PanelMode::Interact;
}

fn open_collection_finder(app: &mut App) {
    app.popup = Popup::CollectionFinder {
        query: single_line_state(""),
        selected: 0,
        browse: false,
        confirm_delete: None,
    };
}

fn handle_method_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => app.current.method = app.current.method.prev(),
        KeyCode::Char('l') | KeyCode::Right => app.current.method = app.current.method.next(),
        _ => {}
    }
}

fn handle_path_params_key(app: &mut App, key: KeyEvent) {
    let len = app.current.path_params.len();
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if len > 0 {
                app.path_param_index = (app.path_param_index + 1).min(len - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.path_param_index = app.path_param_index.saturating_sub(1);
        }
        KeyCode::Char('i') | KeyCode::Enter if len > 0 => {
            open_path_param_form(app, app.path_param_index);
        }
        _ => {}
    }
}

fn handle_headers_key(app: &mut App, key: KeyEvent) {
    let len = app.current.headers.len();
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if len > 0 {
                app.header_index = (app.header_index + 1).min(len - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.header_index = app.header_index.saturating_sub(1);
        }
        KeyCode::Char('n') => {
            open_header_form(app, HeaderFormTarget::RequestHeader, None);
        }
        KeyCode::Char('i') | KeyCode::Enter if len > 0 => {
            open_header_form(app, HeaderFormTarget::RequestHeader, Some(app.header_index));
        }
        KeyCode::Char('d') if len > 0 => {
            app.current.headers.remove(app.header_index);
            app.header_index = app.header_index.saturating_sub(1);
        }
        KeyCode::Char('x') if len > 0 => {
            if let Some(h) = app.selected_header_mut() {
                h.enabled = !h.enabled;
            }
        }
        _ => {}
    }
}

fn cycle_body_mode(app: &mut App, forward: bool) {
    app.current.body_mode = if forward {
        app.current.body_mode.next()
    } else {
        app.current.body_mode.prev()
    };
    app.form_field_index = 0;
}

fn handle_body_field_key(app: &mut App, key: KeyEvent) {
    match app.current.body_mode {
        BodyMode::Raw => unreachable!("Raw body is a vim buffer, handled above"),
        BodyMode::FormData => handle_form_data_key(app, key),
        BodyMode::Binary => handle_binary_key(app, key),
    }
}

fn handle_form_data_key(app: &mut App, key: KeyEvent) {
    let len = app.current.form_fields.len();
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if len > 0 {
                app.form_field_index = (app.form_field_index + 1).min(len - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.form_field_index = app.form_field_index.saturating_sub(1);
        }
        KeyCode::Char('n') => {
            open_header_form(app, HeaderFormTarget::FormField, None);
        }
        KeyCode::Char('i') | KeyCode::Enter if len > 0 => {
            open_header_form(app, HeaderFormTarget::FormField, Some(app.form_field_index));
        }
        KeyCode::Char('d') if len > 0 => {
            app.current.form_fields.remove(app.form_field_index);
            app.form_field_index = app.form_field_index.saturating_sub(1);
        }
        KeyCode::Char('x') if len > 0 => {
            if let Some(f) = app.selected_form_field_mut() {
                f.enabled = !f.enabled;
            }
        }
        KeyCode::Char('t') if len > 0 => toggle_selected_form_field_kind(app),
        _ => {}
    }
}

fn toggle_selected_form_field_kind(app: &mut App) {
    if let Some(f) = app.selected_form_field_mut() {
        f.value = match &f.value {
            FormValue::Text(s) => FormValue::File(s.clone()),
            FormValue::File(s) => FormValue::Text(s.clone()),
        };
    }
}

fn set_form_value_text_preserving_kind(value: &mut FormValue, text: String) {
    match value {
        FormValue::Text(s) | FormValue::File(s) => *s = text,
    }
}

fn handle_binary_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('i') | KeyCode::Enter => {
            app.popup = Popup::TextInput {
                title: "Binary file path".to_string(),
                state: vim_line_state(&app.current.binary_file),
                target: TextInputTarget::BinaryFilePath,
            };
        }
        KeyCode::Char('d') => app.current.binary_file.clear(),
        _ => {}
    }
}

fn current_editor_mode(app: &App) -> EditorMode {
    match app.focus {
        Focus::Url => app.url_state.mode,
        Focus::Body => app.body_state.mode,
        Focus::Response => app.response_state.mode,
        Focus::Console => app.console_state.mode,
        Focus::ScriptEditor => match app.script_tab {
            ScriptTab::Pre => app.pre_script_state.mode,
            ScriptTab::Post => app.post_script_state.mode,
        },
        _ => EditorMode::Normal,
    }
}

fn handle_editor_pane_key(app: &mut App, key: KeyEvent) -> Action {
    if key.code == KeyCode::Esc && current_editor_mode(app) == EditorMode::Normal {
        app.panel_mode = PanelMode::Navigate;
        return Action::None;
    }

    if app.focus == Focus::Body
        && app.body_state.mode == EditorMode::Normal
        && key.code == KeyCode::Char('f')
    {
        format_body(app);
        return Action::None;
    }
    if app.focus == Focus::Response
        && app.response_state.mode == EditorMode::Normal
        && key.code == KeyCode::F(4)
    {
        open_link_near_cursor(app);
        return Action::None;
    }

    if app.focus == Focus::Console {
        handle_console_key(app, key);
        return Action::None;
    }

    if app.var_input().is_some() {
        feed_var_input(app, key);
    } else {
        let state = match app.focus {
            Focus::Response => &mut app.response_state,
            Focus::ScriptEditor => match app.script_tab {
                ScriptTab::Pre => &mut app.pre_script_state,
                ScriptTab::Post => &mut app.post_script_state,
            },
            _ => unreachable!("URL and raw Body are var inputs"),
        };
        app.editor_events.on_key_event(key, state);
    }

    if app.focus == Focus::Url {
        app.sync_current_from_editors();
    }
    Action::None
}

/// Send a key to the focused `{{variable}}`-aware input (see `App::var_input`),
/// with completion handled by the `completion` module.
fn feed_var_input(app: &mut App, key: KeyEvent) {
    let vars = app.active_env_vars();
    let mut menu = app.var_menu.take();
    let mut events = std::mem::take(&mut app.editor_events);
    if let Some(state) = app.var_input() {
        crate::completion::handle_key(state, &mut menu, &vars, key, &mut events);
    }
    app.editor_events = events;
    app.var_menu = menu;
}

/// Console is read-only: vim motions/search/yank work, but any edit is reverted.
fn handle_console_key(app: &mut App, key: KeyEvent) {
    let before = app.console_state.lines.to_string();
    app.editor_events.on_key_event(key, &mut app.console_state);
    let state = &mut app.console_state;
    if state.mode == EditorMode::Insert {
        state.mode = EditorMode::Normal;
    }
    if state.lines.to_string() != before {
        let cursor = state.cursor;
        state.lines = edtui::Lines::from(before.as_str());
        let last_row = state.lines.len().saturating_sub(1);
        let row = cursor.row.min(last_row);
        let col = cursor.col.min(state.lines.len_col(row).unwrap_or(0).saturating_sub(1));
        state.cursor = edtui::Index2::new(row, col);
        app.status_message = "Console is read-only".into();
    }
}

fn format_body(app: &mut App) {
    let current = app.body_state.lines.to_string();
    match crate::format::pretty_json(&current) {
        Some(pretty) => {
            app.body_state = edtui::EditorState::new(edtui::Lines::from(pretty.as_str()));
            app.current.body = pretty;
            app.status_message = "Body formatted as JSON".into();
        }
        None => {
            app.status_message = "Body is not valid JSON — nothing to format".into();
        }
    }
}

fn open_link_near_cursor(app: &mut App) {
    let row = app.response_state.cursor.row;
    let text = app.response_state.lines.to_string();
    let Some(line) = text.lines().nth(row) else {
        return;
    };
    match find_link(line) {
        Some(url) => match open::that(&url) {
            Ok(()) => app.status_message = format!("Opened {url}"),
            Err(e) => {
                crate::logger::log("open link failed", &format!("{url}: {e}"));
                app.status_message = format!("Failed to open {url}: {e}");
            }
        },
        None => app.status_message = "No link on this line".into(),
    }
}

fn find_link(line: &str) -> Option<String> {
    for scheme in ["https://", "http://"] {
        if let Some(start) = line.find(scheme) {
            let rest = &line[start..];
            let end = rest
                .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')' | ']' | '}' | ','))
                .unwrap_or(rest.len());
            let url = &rest[..end];
            if url.len() > scheme.len() {
                return Some(url.to_string());
            }
        }
    }
    None
}

fn open_text_input(app: &mut App, title: &str, target: TextInputTarget) {
    app.popup = Popup::TextInput {
        title: title.to_string(),
        state: vim_line_state(""),
        target,
    };
}

fn open_header_form(app: &mut App, target: HeaderFormTarget, edit_index: Option<usize>) {
    let (k, v) = match (target, edit_index) {
        (HeaderFormTarget::RequestHeader, Some(i)) => {
            let h = &app.current.headers[i];
            (h.key.clone(), h.value.clone())
        }
        (HeaderFormTarget::EnvVariable(env_i), Some(i)) => {
            let e = &app.workspace.environments[env_i];
            let (k, v) = &e.variables[i];
            (k.clone(), v.clone())
        }
        (HeaderFormTarget::FormField, Some(i)) => {
            let f = &app.current.form_fields[i];
            (f.key.clone(), f.value.as_str().to_string())
        }
        _ => (String::new(), String::new()),
    };
    app.popup = Popup::HeaderForm {
        key: vim_line_state(&k),
        value: vim_line_state(&v),
        field: HeaderFormField::Key,
        edit_index,
        target,
        interacting: false,
    };
}

fn open_path_param_form(app: &mut App, index: usize) {
    let Some((name, value)) = app.current.path_params.get(index).cloned() else {
        return;
    };
    app.popup = Popup::PathParamForm {
        name,
        value: vim_line_state(&value),
        index,
    };
}

fn handle_popup_key(app: &mut App, key: KeyEvent) {
    if matches!(app.popup, Popup::CollectionFinder { .. }) {
        handle_collection_finder_key(app, key);
        return;
    }
    if app.var_menu.is_some() && app.var_input().is_some() {
        feed_var_input(app, key);
        return;
    }

    match &mut app.popup {
        Popup::None => {}
        Popup::TextInput { state, target, .. } => {
            // Vim semantics: Esc leaves Insert/Visual/Search first; Esc in Normal cancels.
            if key.code == KeyCode::Esc && state.mode == EditorMode::Normal {
                app.popup = Popup::None;
                return;
            }
            if key.code == KeyCode::Enter {
                let name = state.lines.to_string();
                let target = *target;
                app.popup = Popup::None;
                if name.trim().is_empty() {
                    return;
                }
                match target {
                    TextInputTarget::NewCollectionName => {
                        app.workspace.collections.push(Collection::new(name));
                        let id = app.workspace.collections.last().unwrap().id;
                        app.expanded.insert(id);
                    }
                    TextInputTarget::NewRequestName(ci) => {
                        if let Some(c) = app.workspace.collections.get_mut(ci) {
                            c.requests.push(RequestItem::new(name));
                        }
                    }
                    TextInputTarget::NewEnvName => {
                        app.workspace.environments.push(Environment::new(name));
                        if app.workspace.active_environment.is_none() {
                            app.workspace.active_environment =
                                Some(app.workspace.environments.last().unwrap().id);
                        }
                    }
                    TextInputTarget::BinaryFilePath => {
                        app.current.binary_file = name;
                        return;
                    }
                }
                app.persist();
                return;
            }
            app.editor_events.on_key_event(key, state);
        }
        Popup::HeaderForm {
            key: key_state,
            value: value_state,
            field,
            edit_index,
            target,
            interacting,
        } => {
            let confirm = key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('s') | KeyCode::Enter);

            if confirm {
                let k = key_state.lines.to_string();
                let v = value_state.lines.to_string();
                let edit_index = *edit_index;
                let target = *target;
                app.popup = Popup::None;
                if k.trim().is_empty() {
                    return;
                }
                match target {
                    HeaderFormTarget::RequestHeader => match edit_index {
                        Some(i) => {
                            app.current.headers[i].key = k;
                            app.current.headers[i].value = v;
                        }
                        None => app.current.headers.push(Header::new(k, v)),
                    },
                    HeaderFormTarget::EnvVariable(env_i) => {
                        if let Some(env) = app.workspace.environments.get_mut(env_i) {
                            match edit_index {
                                Some(i) => env.variables[i] = (k, v),
                                None => env.variables.push((k, v)),
                            }
                        }
                        app.persist();
                    }
                    HeaderFormTarget::FormField => match edit_index {
                        Some(i) => {
                            app.current.form_fields[i].key = k;
                            set_form_value_text_preserving_kind(&mut app.current.form_fields[i].value, v);
                        }
                        None => app.current.form_fields.push(FormField::new_text(k, v)),
                    },
                }
                return;
            }

            if !*interacting {
                // Navigate mode: same keys as the main UI.
                match key.code {
                    KeyCode::Esc => app.popup = Popup::None,
                    KeyCode::Char('k') | KeyCode::Char('h') | KeyCode::Up | KeyCode::Left => {
                        *field = HeaderFormField::Key
                    }
                    KeyCode::Char('j') | KeyCode::Char('l') | KeyCode::Down | KeyCode::Right => {
                        *field = HeaderFormField::Value
                    }
                    KeyCode::Enter => *interacting = true,
                    KeyCode::Char('i') => {
                        *interacting = true;
                        match field {
                            HeaderFormField::Key => key_state.mode = EditorMode::Insert,
                            HeaderFormField::Value => value_state.mode = EditorMode::Insert,
                        }
                    }
                    _ => {}
                }
                return;
            }

            let state = match field {
                HeaderFormField::Key => &mut *key_state,
                HeaderFormField::Value => &mut *value_state,
            };
            if key.code == KeyCode::Esc && state.mode == EditorMode::Normal {
                *interacting = false;
                return;
            }
            feed_var_input(app, key);
        }
        Popup::EnvManager => {
            let len = app.workspace.environments.len();
            match key.code {
                KeyCode::Esc => app.popup = Popup::None,
                KeyCode::Char('j') | KeyCode::Down => {
                    if len > 0 {
                        app.env_index = (app.env_index + 1).min(len - 1);
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    app.env_index = app.env_index.saturating_sub(1);
                }
                KeyCode::Char('n') => {
                    open_text_input(app, "New environment name", TextInputTarget::NewEnvName);
                }
                KeyCode::Enter if len > 0 => {
                    app.workspace.active_environment = Some(app.workspace.environments[app.env_index].id);
                    app.persist();
                }
                KeyCode::Char('v') if len > 0 => {
                    app.env_var_index = 0;
                    app.popup = Popup::EnvVarEditor {
                        env_index: app.env_index,
                    };
                }
                KeyCode::Char('d') if len > 0 => {
                    let removed_id = app.workspace.environments[app.env_index].id;
                    app.workspace.environments.remove(app.env_index);
                    if app.workspace.active_environment == Some(removed_id) {
                        app.workspace.active_environment = None;
                    }
                    app.env_index = app.env_index.saturating_sub(1);
                    app.persist();
                }
                _ => {}
            }
        }
        Popup::EnvVarEditor { env_index } => {
            let env_index = *env_index;
            let len = app
                .workspace
                .environments
                .get(env_index)
                .map(|e| e.variables.len())
                .unwrap_or(0);
            match key.code {
                KeyCode::Esc => app.popup = Popup::EnvManager,
                KeyCode::Char('j') | KeyCode::Down => {
                    if len > 0 {
                        app.env_var_index = (app.env_var_index + 1).min(len - 1);
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    app.env_var_index = app.env_var_index.saturating_sub(1);
                }
                KeyCode::Char('n') => {
                    open_header_form(app, HeaderFormTarget::EnvVariable(env_index), None);
                }
                KeyCode::Char('i') | KeyCode::Enter if len > 0 => {
                    open_header_form(
                        app,
                        HeaderFormTarget::EnvVariable(env_index),
                        Some(app.env_var_index),
                    );
                }
                KeyCode::Char('d') if len > 0 => {
                    app.workspace.environments[env_index]
                        .variables
                        .remove(app.env_var_index);
                    app.env_var_index = app.env_var_index.saturating_sub(1);
                    app.persist();
                }
                _ => {}
            }
        }
        Popup::PathParamForm { value, index, .. } => {
            // Vim semantics: Esc leaves Insert first; Esc in Normal cancels.
            if key.code == KeyCode::Esc && value.mode == EditorMode::Normal {
                app.popup = Popup::None;
                return;
            }
            if key.code == KeyCode::Enter {
                let v = value.lines.to_string();
                let index = *index;
                app.popup = Popup::None;
                if let Some((_, existing)) = app.current.path_params.get_mut(index) {
                    *existing = v;
                }
                return;
            }
            feed_var_input(app, key);
        }
        Popup::History { selected } => {
            let len = app.history.len();
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => app.popup = Popup::None,
                KeyCode::Char('j') | KeyCode::Down if len > 0 => *selected = (*selected + 1).min(len - 1),
                KeyCode::Char('k') | KeyCode::Up => *selected = selected.saturating_sub(1),
                KeyCode::Char('g') => *selected = 0,
                KeyCode::Char('G') => *selected = len.saturating_sub(1),
                KeyCode::Enter => {
                    // Shown newest first.
                    let entry = len.checked_sub(*selected + 1).and_then(|i| app.history.get(i));
                    let target = entry.and_then(|e| e.request_id).and_then(|id| app.find_request(id));
                    match target {
                        Some((ci, ri)) => {
                            app.popup = Popup::None;
                            app.load_request(ci, ri);
                        }
                        None => app.status_message = "That request no longer exists".into(),
                    }
                }
                _ => {}
            }
        }
        Popup::CollectionFinder { .. } => unreachable!("handled above"),
    }
}

fn collection_finder_selected(app: &App) -> usize {
    match &app.popup {
        Popup::CollectionFinder { selected, .. } => *selected,
        _ => 0,
    }
}

fn collection_finder_browse(app: &App) -> bool {
    matches!(&app.popup, Popup::CollectionFinder { browse: true, .. })
}

fn set_collection_finder_browse(app: &mut App, value: bool) {
    if let Popup::CollectionFinder { browse, selected, .. } = &mut app.popup {
        *browse = value;
        *selected = 0;
    }
}

fn collection_finder_query_text(app: &App) -> String {
    match &app.popup {
        Popup::CollectionFinder { query, .. } => query.lines.to_string(),
        _ => String::new(),
    }
}

fn set_collection_finder_selected(app: &mut App, value: usize) {
    if let Popup::CollectionFinder { selected, .. } = &mut app.popup {
        *selected = value;
    }
}

fn select_next_collection_finder_match(app: &mut App, selected: usize, match_count: usize) {
    if match_count > 0 {
        set_collection_finder_selected(app, (selected + 1).min(match_count - 1));
    }
}

fn select_prev_collection_finder_match(app: &mut App, selected: usize) {
    set_collection_finder_selected(app, selected.saturating_sub(1));
}

fn finder_activate(app: &mut App, entry: Option<SidebarEntry>) {
    match entry {
        Some(SidebarEntry::Collection(ci)) => {
            let id = app.workspace.collections[ci].id;
            if !app.expanded.remove(&id) {
                app.expanded.insert(id);
            }
        }
        Some(SidebarEntry::Request(ci, ri)) => {
            app.load_request(ci, ri);
            app.popup = Popup::None;
        }
        None => {}
    }
}

fn finder_new_request(app: &mut App, entry: Option<SidebarEntry>) {
    if let Some(SidebarEntry::Collection(ci) | SidebarEntry::Request(ci, _)) = entry {
        open_text_input(app, "New request name", TextInputTarget::NewRequestName(ci));
    }
}

fn request_finder_delete(app: &mut App, entry: Option<SidebarEntry>) {
    if let Popup::CollectionFinder { confirm_delete, .. } = &mut app.popup {
        *confirm_delete = entry;
    }
}

fn handle_collection_finder_key(app: &mut App, key: KeyEvent) {
    // A pending delete swallows the next key: y confirms, anything else cancels.
    let pending = match &mut app.popup {
        Popup::CollectionFinder { confirm_delete, .. } => confirm_delete.take(),
        _ => None,
    };
    if let Some(entry) = pending {
        if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            delete_collection_finder_selection(app, Some(entry));
        } else {
            app.status_message = "Delete cancelled".into();
        }
        return;
    }

    let browse = collection_finder_browse(app);

    if key.code == KeyCode::Esc {
        // Vim-like: Esc leaves the search input for the list; Esc in the list closes.
        if browse {
            app.popup = Popup::None;
        } else {
            set_collection_finder_browse(app, true);
        }
        return;
    }

    let selected = collection_finder_selected(app);
    let matches = app.collection_finder_matches(&collection_finder_query_text(app), browse);
    let current = matches.get(selected).copied();

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('n') => {
                select_next_collection_finder_match(app, selected, matches.len());
                return;
            }
            KeyCode::Char('p') => {
                select_prev_collection_finder_match(app, selected);
                return;
            }
            KeyCode::Char('r') => {
                finder_new_request(app, current);
                return;
            }
            KeyCode::Char('a') => {
                open_text_input(app, "New collection name", TextInputTarget::NewCollectionName);
                return;
            }
            KeyCode::Char('d') => {
                request_finder_delete(app, current);
                return;
            }
            _ => {}
        }
    }

    if browse {
        handle_collection_browse_key(app, key, selected, &matches);
        return;
    }

    match key.code {
        KeyCode::Down => select_next_collection_finder_match(app, selected, matches.len()),
        KeyCode::Up => select_prev_collection_finder_match(app, selected),
        KeyCode::Enter => finder_activate(app, current),
        _ => {
            if let Popup::CollectionFinder { query, .. } = &mut app.popup {
                app.editor_events.on_key_event(key, query);
            }
            let new_len = app
                .collection_finder_matches(&collection_finder_query_text(app), false)
                .len();
            set_collection_finder_selected(app, selected.min(new_len.saturating_sub(1)));
        }
    }
}

/// List-focused ("browse") keys: vim-style navigation over the collection tree.
fn handle_collection_browse_key(app: &mut App, key: KeyEvent, selected: usize, matches: &[SidebarEntry]) {
    let current = matches.get(selected).copied();
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => select_next_collection_finder_match(app, selected, matches.len()),
        KeyCode::Char('k') | KeyCode::Up => select_prev_collection_finder_match(app, selected),
        KeyCode::Char('g') => set_collection_finder_selected(app, 0),
        KeyCode::Char('G') => set_collection_finder_selected(app, matches.len().saturating_sub(1)),
        KeyCode::Char('/') | KeyCode::Char('i') => set_collection_finder_browse(app, false),
        KeyCode::Enter => finder_activate(app, current),
        KeyCode::Char('l') | KeyCode::Right => match current {
            Some(SidebarEntry::Collection(ci)) => {
                let id = app.workspace.collections[ci].id;
                app.expanded.insert(id);
            }
            Some(SidebarEntry::Request(..)) => finder_activate(app, current),
            None => {}
        },
        KeyCode::Char('h') | KeyCode::Left => match current {
            Some(SidebarEntry::Collection(ci)) => {
                let id = app.workspace.collections[ci].id;
                app.expanded.remove(&id);
            }
            Some(SidebarEntry::Request(ci, _)) => {
                if let Some(pos) = matches.iter().position(|e| *e == SidebarEntry::Collection(ci)) {
                    set_collection_finder_selected(app, pos);
                }
            }
            None => {}
        },
        KeyCode::Char('a') => finder_new_request(app, current),
        KeyCode::Char('A') => open_text_input(app, "New collection name", TextInputTarget::NewCollectionName),
        KeyCode::Char('d') => request_finder_delete(app, current),
        _ => {}
    }
}

fn delete_collection_finder_selection(app: &mut App, entry: Option<SidebarEntry>) {
    match entry {
        Some(SidebarEntry::Collection(ci)) => {
            for r in &app.workspace.collections[ci].requests {
                app.drafts.remove(&r.id);
                app.response_history.remove(&r.id);
            }
            app.workspace.collections.remove(ci);
            app.persist();
        }
        Some(SidebarEntry::Request(ci, ri)) => {
            let removed = app.workspace.collections[ci].requests.remove(ri);
            app.drafts.remove(&removed.id);
            app.response_history.remove(&removed.id);
            if app.source == Some(RequestSource { collection_index: ci, request_index: ri }) {
                app.source = None;
            }
            app.persist();
        }
        None => {}
    }
    app.prune_tabs();
    let selected = collection_finder_selected(app).saturating_sub(1);
    set_collection_finder_selected(app, selected);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Workspace;
    use crate::test_support::isolated_storage;

    fn app_with_env() -> App {
        let mut ws = Workspace::default();
        let mut env = Environment::new("dev");
        env.variables.push(("token".into(), "abc".into()));
        ws.active_environment = Some(env.id);
        ws.environments.push(env);
        App::new(ws)
    }

    fn press(app: &mut App, code: KeyCode) {
        let mods = if matches!(code, KeyCode::Char(c) if c.is_ascii_uppercase()) {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        handle_key(app, KeyEvent::new(code, mods));
    }

    fn typ(app: &mut App, text: &str) {
        text.chars().for_each(|c| press(app, KeyCode::Char(c)));
    }

    #[test]
    fn url_input_offers_variable_completion() {
        let _storage = isolated_storage();
        let mut app = app_with_env();
        app.focus = Focus::Url;
        app.panel_mode = PanelMode::Interact;
        typ(&mut app, "A{{to");
        assert!(app.var_menu.is_some());
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.url_state.lines.to_string(), "{{token}}");
        assert_eq!(app.current.url, "{{token}}", "URL edits sync into the request");
    }

    #[test]
    fn path_param_popup_is_a_vim_input_with_completion() {
        let _storage = isolated_storage();
        let mut app = app_with_env();
        app.current.path_params = vec![("id".into(), String::new())];
        open_path_param_form(&mut app, 0);
        let Popup::PathParamForm { value, .. } = &app.popup else { panic!() };
        assert_eq!(value.mode, EditorMode::Normal, "opens in Normal like the other popups");

        typ(&mut app, "i{{");
        press(&mut app, KeyCode::Enter); // picks the variable, doesn't confirm the popup
        assert!(matches!(app.popup, Popup::PathParamForm { .. }));
        press(&mut app, KeyCode::Enter); // now confirms
        assert_eq!(app.current.path_params[0].1, "{{token}}");

        open_path_param_form(&mut app, 0);
        typ(&mut app, "A!");
        press(&mut app, KeyCode::Esc); // Insert -> Normal
        assert!(matches!(app.popup, Popup::PathParamForm { .. }));
        press(&mut app, KeyCode::Esc); // Normal -> cancel
        assert!(matches!(app.popup, Popup::None));
        assert_eq!(app.current.path_params[0].1, "{{token}}", "cancel keeps the old value");
    }

    #[test]
    fn m_then_digit_in_tabs_assigns_and_space_digit_opens() {
        let _storage = isolated_storage(); // assigning a slot saves the workspace
        let mut ws = Workspace::default();
        let mut c = crate::model::Collection::new("c");
        c.requests.push(RequestItem::new("basket"));
        c.requests.push(RequestItem::new("upload"));
        ws.collections.push(c);
        let mut app = App::new(ws);
        let (basket, upload) = (app.workspace.collections[0].requests[0].id, app.workspace.collections[0].requests[1].id);
        app.workspace.open_tabs = vec![basket, upload];
        app.focus = Focus::Tabs;
        app.panel_mode = PanelMode::Interact;

        app.tab_index = 1;
        typ(&mut app, "m3");
        app.tab_index = 0;
        typ(&mut app, "m1");
        assert_eq!(app.slot_of(basket), Some(1));
        assert_eq!(app.slot_of(upload), Some(3));

        typ(&mut app, "mx"); // anything but a digit cancels
        assert_eq!(app.slot_of(basket), Some(1));
        assert!(!app.tab_mark_pending);

        typ(&mut app, "m0");
        assert_eq!(app.slot_of(basket), None);
    }
}
