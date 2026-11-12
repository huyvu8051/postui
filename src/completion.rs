//! `{{variable}}` completion for the vim inputs whose text gets environment
//! substitution (URL, raw Body, header/form key-value fields, path param values).
//!
//! In Insert mode:
//! - typing `{{` auto-closes it to `{{|}}` and opens a menu of the active
//!   environment's variables, filtered by what's typed between the braces;
//! - `Ctrl+N`/`Ctrl+P` (or `↓`/`↑`) move, `Tab`/`Enter` insert the variable and
//!   jump past `}}`, `Esc` closes the menu (a second `Esc` leaves Insert mode);
//! - `Ctrl+Space` reopens the menu while the cursor is inside `{{…}}`;
//! - `Backspace` inside an empty `{{|}}` deletes the whole pair.
//!
//! Edits are made straight on the current line during the insert session, so a
//! single `u` undoes them together with the typed text.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use edtui::{EditorEventHandler, EditorMode, EditorState, RowIndex};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

/// An open completion menu; `selected` indexes [`candidates`].
#[derive(Debug, Default, Clone, Copy)]
pub struct VarMenu {
    pub selected: usize,
}

/// Route one key to a variable-aware input: the menu gets first pick, then
/// edtui, then the brace/menu bookkeeping.
pub fn handle_key(
    state: &mut EditorState,
    menu: &mut Option<VarMenu>,
    vars: &[(String, String)],
    key: KeyEvent,
    events: &mut EditorEventHandler,
) {
    if state.mode != EditorMode::Insert || prefix_at_cursor(state).is_none() {
        *menu = None; // stale: left Insert mode, moved out of the braces, or another input
    }

    if let Some(open) = menu.as_mut() {
        match handle_menu_key(state, open, vars, key) {
            MenuKey::Handled => return,
            MenuKey::Close => {
                *menu = None;
                return;
            }
            MenuKey::NotMine => {}
        }
    }

    if state.mode == EditorMode::Insert {
        if is_ctrl(key, ' ') {
            if prefix_at_cursor(state).is_some() {
                *menu = Some(VarMenu::default());
            }
            return;
        }
        if key.code == KeyCode::Backspace && delete_empty_pair(state) {
            *menu = None;
            return;
        }
    }

    events.on_key_event(key, state);

    if state.mode == EditorMode::Insert
        && key.code == KeyCode::Char('{')
        && auto_close_braces(state)
    {
        *menu = Some(VarMenu::default());
    }
    match menu {
        Some(_) if state.mode != EditorMode::Insert || prefix_at_cursor(state).is_none() => {
            *menu = None
        }
        Some(open) => open.selected = 0, // the filter text changed
        None => {}
    }
}

enum MenuKey {
    /// Consumed; the menu stays open (moving the selection).
    Handled,
    /// Consumed; the menu closes (picked a variable, or Esc).
    Close,
    /// Not a menu key: let edtui have it.
    NotMine,
}

fn handle_menu_key(
    state: &mut EditorState,
    menu: &mut VarMenu,
    vars: &[(String, String)],
    key: KeyEvent,
) -> MenuKey {
    let matches = candidates(vars, &prefix_at_cursor(state).unwrap_or_default());
    if key.code == KeyCode::Down || is_ctrl(key, 'n') {
        menu.selected = (menu.selected + 1).min(matches.len().saturating_sub(1));
        return MenuKey::Handled;
    }
    if key.code == KeyCode::Up || is_ctrl(key, 'p') {
        menu.selected = menu.selected.saturating_sub(1);
        return MenuKey::Handled;
    }
    match key.code {
        KeyCode::Esc => MenuKey::Close,
        KeyCode::Enter | KeyCode::Tab => match matches.get(menu.selected) {
            Some((name, _)) => {
                accept(state, name);
                MenuKey::Close
            }
            None => MenuKey::NotMine, // nothing to pick: behave like a normal Enter/Tab
        },
        _ => MenuKey::NotMine,
    }
}

/// Active-environment variables matching what's typed inside the braces, best first.
pub fn candidates<'a>(vars: &'a [(String, String)], prefix: &str) -> Vec<&'a (String, String)> {
    if prefix.is_empty() {
        return vars.iter().collect();
    }
    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<_> = vars
        .iter()
        .filter_map(|v| matcher.fuzzy_match(&v.0, prefix).map(|score| (score, v)))
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, v)| v).collect()
}

/// What's typed between `{{` and the cursor, when the cursor sits inside an
/// unfinished `{{…`. None when not inside one.
pub fn prefix_at_cursor(state: &EditorState) -> Option<String> {
    let line = current_line(state)?;
    let before = &line[..state.cursor.col.min(line.len())];
    let open = before.windows(2).rposition(|w| w == ['{', '{'])?;
    let typed = &before[open + 2..];
    if typed
        .iter()
        .any(|c| matches!(c, '{' | '}') || c.is_whitespace())
    {
        return None;
    }
    Some(typed.iter().collect())
}

/// Right after a `{` was typed: turn `{{|` into `{{|}}`. Returns true if it did.
fn auto_close_braces(state: &mut EditorState) -> bool {
    let col = state.cursor.col;
    let Some(line) = current_line_mut(state) else {
        return false;
    };
    let just_opened = col >= 2 && line[col - 2..col] == ['{', '{'];
    let already_closed = line[col..].starts_with(&['}', '}']);
    if !just_opened || already_closed {
        return false;
    }
    line.splice(col..col, ['}', '}']);
    true
}

/// Backspace inside an empty `{{|}}`: remove all four braces. Returns true if it did.
fn delete_empty_pair(state: &mut EditorState) -> bool {
    let col = state.cursor.col;
    let Some(line) = current_line_mut(state) else {
        return false;
    };
    let empty_pair =
        col >= 2 && line[col - 2..col] == ['{', '{'] && line[col..].starts_with(&['}', '}']);
    if !empty_pair {
        return false;
    }
    line.drain(col - 2..col + 2);
    state.cursor.col = col - 2;
    true
}

/// Replace the typed prefix with `name` and put the cursor after the closing `}}`.
fn accept(state: &mut EditorState, name: &str) {
    let Some(prefix_len) = prefix_at_cursor(state).map(|p| p.chars().count()) else {
        return;
    };
    let col = state.cursor.col;
    let start = col - prefix_len;
    let Some(line) = current_line_mut(state) else {
        return;
    };
    line.splice(start..col, name.chars());
    let mut end = start + name.chars().count();
    if !line[end..].starts_with(&['}', '}']) {
        line.splice(end..end, ['}', '}']);
    }
    end += 2;
    state.cursor.col = end;
}

fn current_line(state: &EditorState) -> Option<&Vec<char>> {
    state.lines.get(RowIndex::new(state.cursor.row))
}

fn current_line_mut(state: &mut EditorState) -> Option<&mut Vec<char>> {
    state.lines.get_mut(RowIndex::new(state.cursor.row))
}

fn is_ctrl(key: KeyEvent, c: char) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use edtui::Lines;

    struct Input {
        state: EditorState,
        menu: Option<VarMenu>,
        events: EditorEventHandler,
        vars: Vec<(String, String)>,
    }

    impl Input {
        /// A single-line input (like the URL) already in Insert mode at the end.
        fn new(text: &str) -> Self {
            let mut state = EditorState::new(Lines::from(text));
            state.set_single_line(true);
            let mut input = Input {
                state,
                menu: None,
                events: EditorEventHandler::default(),
                vars: vec![
                    ("baseUrl".into(), "https://api".into()),
                    ("token".into(), "abc".into()),
                    ("tenantId".into(), "t1".into()),
                ],
            };
            input.key(KeyCode::Char('A'), KeyModifiers::SHIFT);
            input
        }

        fn key(&mut self, code: KeyCode, mods: KeyModifiers) {
            handle_key(
                &mut self.state,
                &mut self.menu,
                &self.vars,
                KeyEvent::new(code, mods),
                &mut self.events,
            );
        }

        fn typ(&mut self, text: &str) {
            for c in text.chars() {
                self.key(KeyCode::Char(c), KeyModifiers::NONE);
            }
        }

        fn text(&self) -> String {
            self.state.lines.to_string()
        }

        /// Text with `|` marking the cursor.
        fn at_cursor(&self) -> String {
            let mut t: Vec<char> = self.text().chars().collect();
            t.insert(self.state.cursor.col, '|');
            t.into_iter().collect()
        }
    }

    #[test]
    fn double_brace_auto_closes_and_opens_the_menu() {
        let mut i = Input::new("x/");
        i.typ("{{");
        assert_eq!(i.at_cursor(), "x/{{|}}");
        assert!(i.menu.is_some());
    }

    #[test]
    fn typing_filters_and_tab_inserts_the_variable_past_the_braces() {
        let mut i = Input::new("x/");
        i.typ("{{t");
        let names: Vec<_> = candidates(&i.vars, "t")
            .iter()
            .map(|v| v.0.clone())
            .collect();
        assert!(names.contains(&"token".to_string()) && names.contains(&"tenantId".to_string()));

        i.typ("ok");
        assert_eq!(candidates(&i.vars, "tok")[0].0, "token");
        i.key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(i.at_cursor(), "x/{{token}}|");
        assert!(i.menu.is_none());
    }

    #[test]
    fn ctrl_n_moves_and_enter_picks() {
        let mut i = Input::new("");
        i.typ("{{");
        i.key(KeyCode::Char('n'), KeyModifiers::CONTROL);
        i.key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(i.text(), "{{token}}");
    }

    #[test]
    fn esc_closes_menu_first_then_leaves_insert() {
        let mut i = Input::new("");
        i.typ("{{");
        i.key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(i.menu.is_none());
        assert_eq!(i.state.mode, EditorMode::Insert);
        i.key(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(i.state.mode, EditorMode::Normal);
    }

    #[test]
    fn ctrl_space_reopens_inside_braces_only() {
        let mut i = Input::new("a");
        i.key(KeyCode::Char(' '), KeyModifiers::CONTROL);
        assert!(i.menu.is_none(), "not inside {{…}}");
        i.typ("{{to");
        i.key(KeyCode::Esc, KeyModifiers::NONE);
        i.key(KeyCode::Char(' '), KeyModifiers::CONTROL);
        assert!(i.menu.is_some());
    }

    #[test]
    fn backspace_in_empty_pair_removes_all_braces() {
        let mut i = Input::new("a");
        i.typ("{{");
        i.key(KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(i.at_cursor(), "a|");
        assert!(i.menu.is_none());
    }

    #[test]
    fn existing_closing_braces_are_not_doubled() {
        let mut i = Input::new("}}");
        // cursor to the start, then type `{{` in front of the existing `}}`
        i.key(KeyCode::Esc, KeyModifiers::NONE);
        i.key(KeyCode::Char('0'), KeyModifiers::NONE);
        i.key(KeyCode::Char('i'), KeyModifiers::NONE);
        i.typ("{{");
        assert_eq!(i.at_cursor(), "{{|}}");
    }

    #[test]
    fn one_undo_removes_the_whole_completion() {
        let mut i = Input::new("x");
        i.typ("{{tok");
        i.key(KeyCode::Tab, KeyModifiers::NONE);
        i.key(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(i.text(), "x{{token}}");
        i.key(KeyCode::Char('u'), KeyModifiers::NONE);
        assert_eq!(i.text(), "x");
    }
}
