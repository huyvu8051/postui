use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use edtui::EditorMode;

use crate::app::{
    single_line_state, vim_line_state, App, Direction, Focus, HeaderFormField, HeaderFormTarget, LeaderState,
    PanelMode, Popup, RequestSource, ScriptTab, SidebarEntry, TextInputTarget,
};
use crate::model::{BodyMode, Collection, Environment, FormField, FormValue, Header, RequestItem};

fn is_vim_editor_focus(app: &App) -> bool {
    matches!(app.focus, Focus::Url | Focus::Response | Focus::ScriptEditor)
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

    if let Some(action) = handle_always_global_key(app, key) {
        return action;
    }

    if app.panel_mode == PanelMode::Interact && is_vim_editor_focus(app) {
        return handle_editor_pane_key(app, key);
    }

    if handle_leader_key(app, key) {
        return Action::None;
    }

    match key.code {
        KeyCode::Char('q') => return Action::Quit,
        KeyCode::Char('e') => {
            app.popup = Popup::EnvManager;
            app.env_index = 0;
            return Action::None;
        }
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
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
        app.sync_current_from_editors();
        app.save_current();
        return Some(Action::None);
    }
    if key.code == KeyCode::F(3) {
        app.maximized = !app.maximized;
        app.status_message = if app.maximized {
            "Panel maximized — F3 to restore layout".into()
        } else {
            "Layout restored".into()
        };
        return Some(Action::None);
    }
    None
}

fn handle_navigate_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => app.move_focus(Direction::Left),
        KeyCode::Char('j') | KeyCode::Down => app.move_focus(Direction::Down),
        KeyCode::Char('k') | KeyCode::Up => app.move_focus(Direction::Up),
        KeyCode::Char('l') | KeyCode::Right => app.move_focus(Direction::Right),
        KeyCode::Enter => app.panel_mode = PanelMode::Interact,
        _ => {}
    }
}

fn handle_interact_key(app: &mut App, key: KeyEvent) {
    if key.code == KeyCode::Esc {
        app.panel_mode = PanelMode::Navigate;
        return;
    }

    match app.focus {
        Focus::Method => handle_method_key(app, key),
        Focus::PathParams => handle_path_params_key(app, key),
        Focus::Headers => handle_headers_key(app, key),
        Focus::BodyMode => handle_body_mode_key(app, key),
        Focus::Body => handle_body_field_key(app, key),
        Focus::ScriptTab => handle_script_tab_key(app, key),
        Focus::Url | Focus::Response | Focus::ScriptEditor => unreachable!("handled above"),
    }
}

fn handle_leader_key(app: &mut App, key: KeyEvent) -> bool {
    match (app.leader, key.code) {
        (LeaderState::Idle, KeyCode::Char(' ')) => {
            app.leader = LeaderState::AwaitingS;
            true
        }
        (LeaderState::AwaitingS, KeyCode::Char('s')) => {
            app.leader = LeaderState::AwaitingF;
            true
        }
        (LeaderState::AwaitingF, KeyCode::Char('f')) => {
            app.leader = LeaderState::Idle;
            open_collection_finder(app);
            true
        }
        (LeaderState::Idle, _) => false,
        _ => {
            app.leader = LeaderState::Idle;
            true
        }
    }
}

fn open_collection_finder(app: &mut App) {
    app.popup = Popup::CollectionFinder {
        query: single_line_state(""),
        selected: 0,
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

fn handle_body_mode_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => {
            app.current.body_mode = app.current.body_mode.prev();
            app.form_field_index = 0;
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.current.body_mode = app.current.body_mode.next();
            app.form_field_index = 0;
        }
        _ => {}
    }
}

fn handle_script_tab_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => app.script_tab = app.script_tab.prev(),
        KeyCode::Char('l') | KeyCode::Right => app.script_tab = app.script_tab.next(),
        _ => {}
    }
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

    let state = match app.focus {
        Focus::Url => &mut app.url_state,
        Focus::Body => &mut app.body_state,
        Focus::Response => &mut app.response_state,
        Focus::ScriptEditor => match app.script_tab {
            ScriptTab::Pre => &mut app.pre_script_state,
            ScriptTab::Post => &mut app.post_script_state,
        },
        _ => unreachable!(),
    };
    app.editor_events.on_key_event(key, state);

    if app.focus == Focus::Url {
        app.sync_current_from_editors();
    }
    Action::None
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
        value: single_line_state(&value),
        index,
    };
}

fn handle_popup_key(app: &mut App, key: KeyEvent) {
    if matches!(app.popup, Popup::CollectionFinder { .. }) {
        handle_collection_finder_key(app, key);
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
            app.editor_events.on_key_event(key, state);
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
            if key.code == KeyCode::Esc {
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
            app.editor_events.on_key_event(key, value);
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

fn handle_collection_finder_key(app: &mut App, key: KeyEvent) {
    if key.code == KeyCode::Esc {
        app.popup = Popup::None;
        return;
    }

    let selected = collection_finder_selected(app);
    let matches = app.collection_finder_matches(&collection_finder_query_text(app));

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
                if let Some(SidebarEntry::Collection(ci) | SidebarEntry::Request(ci, _)) =
                    matches.get(selected).copied()
                {
                    open_text_input(app, "New request name", TextInputTarget::NewRequestName(ci));
                }
                return;
            }
            KeyCode::Char('a') => {
                open_text_input(app, "New collection name", TextInputTarget::NewCollectionName);
                return;
            }
            KeyCode::Char('d') => {
                delete_collection_finder_selection(app, matches.get(selected).copied());
                return;
            }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Down => select_next_collection_finder_match(app, selected, matches.len()),
        KeyCode::Up => select_prev_collection_finder_match(app, selected),
        KeyCode::Enter => match matches.get(selected).copied() {
            Some(SidebarEntry::Collection(ci)) => {
                let id = app.workspace.collections[ci].id;
                if app.expanded.contains(&id) {
                    app.expanded.remove(&id);
                } else {
                    app.expanded.insert(id);
                }
            }
            Some(SidebarEntry::Request(ci, ri)) => {
                app.load_request(ci, ri);
                app.popup = Popup::None;
            }
            None => {}
        },
        _ => {
            if let Popup::CollectionFinder { query, .. } = &mut app.popup {
                app.editor_events.on_key_event(key, query);
            }
            let new_len = app.collection_finder_matches(&collection_finder_query_text(app)).len();
            set_collection_finder_selected(app, selected.min(new_len.saturating_sub(1)));
        }
    }
}

fn delete_collection_finder_selection(app: &mut App, entry: Option<SidebarEntry>) {
    match entry {
        Some(SidebarEntry::Collection(ci)) => {
            for r in &app.workspace.collections[ci].requests {
                app.drafts.remove(&r.id);
            }
            app.workspace.collections.remove(ci);
            app.persist();
        }
        Some(SidebarEntry::Request(ci, ri)) => {
            let removed = app.workspace.collections[ci].requests.remove(ri);
            app.drafts.remove(&removed.id);
            if app.source == Some(RequestSource { collection_index: ci, request_index: ri }) {
                app.source = None;
            }
            app.persist();
        }
        None => {}
    }
    let selected = collection_finder_selected(app).saturating_sub(1);
    set_collection_finder_selected(app, selected);
}
