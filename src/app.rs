use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use edtui::{EditorEventHandler, EditorMode, EditorState, Lines};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use uuid::Uuid;

use crate::model::{Environment, FormField, Header, HistoryEntry, RequestItem, ResponseData, Workspace};
use crate::session::{EditorBuffers, StoredDraft, StoredSession};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tabs,
    Method,
    Url,
    PathParams,
    Headers,
    Body,
    ScriptEditor,
    Console,
    Response,
}

impl Focus {
    pub fn neighbor(self, dir: Direction) -> Option<Focus> {
        use Direction::{Down, Left, Right, Up};
        match (self, dir) {
            (Focus::Tabs, Right) => Some(Focus::Method),
            (Focus::Method, Left) => Some(Focus::Tabs),
            (Focus::PathParams, Left) => Some(Focus::Tabs),
            (Focus::Headers, Left) => Some(Focus::Tabs),
            (Focus::Body, Left) => Some(Focus::Tabs),
            (Focus::Method, Right) => Some(Focus::Url),
            (Focus::Method, Down) => Some(Focus::PathParams),
            (Focus::Url, Left) => Some(Focus::Method),
            (Focus::Url, Down) => Some(Focus::PathParams),
            (Focus::PathParams, Up) => Some(Focus::Method),
            (Focus::PathParams, Down) => Some(Focus::Headers),
            (Focus::Headers, Up) => Some(Focus::PathParams),
            (Focus::Headers, Down) => Some(Focus::Body),
            (Focus::Headers, Right) => Some(Focus::ScriptEditor),
            (Focus::Body, Up) => Some(Focus::Headers),
            (Focus::Body, Right) => Some(Focus::Response),
            (Focus::ScriptEditor, Left) => Some(Focus::Headers),
            (Focus::ScriptEditor, Up) => Some(Focus::PathParams),
            (Focus::ScriptEditor, Down) => Some(Focus::Console),
            (Focus::Console, Up) => Some(Focus::ScriptEditor),
            (Focus::Console, Left) => Some(Focus::Headers),
            (Focus::Console, Down) => Some(Focus::Response),
            (Focus::Response, Left) => Some(Focus::Body),
            (Focus::Response, Up) => Some(Focus::Console),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelMode {
    #[default]
    Navigate,
    Interact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScriptTab {
    #[default]
    Pre,
    Post,
}

impl ScriptTab {
    pub fn next(self) -> ScriptTab {
        match self {
            ScriptTab::Pre => ScriptTab::Post,
            ScriptTab::Post => ScriptTab::Pre,
        }
    }

    pub fn prev(self) -> ScriptTab {
        self.next()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarEntry {
    Collection(usize),
    Request(usize, usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeaderState {
    #[default]
    Idle,
    /// `space` pressed; the next key picks the action.
    Awaiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputTarget {
    NewCollectionName,
    NewRequestName(usize),
    NewEnvName,
    BinaryFilePath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderFormField {
    Key,
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderFormTarget {
    RequestHeader,
    EnvVariable(usize),
    FormField,
}

pub enum Popup {
    None,
    TextInput {
        title: String,
        state: EditorState,
        target: TextInputTarget,
    },
    HeaderForm {
        key: EditorState,
        value: EditorState,
        field: HeaderFormField,
        edit_index: Option<usize>,
        target: HeaderFormTarget,
        /// false = navigate between inputs with hjkl; true = typing into the focused input (vim).
        interacting: bool,
    },
    EnvManager,
    EnvVarEditor {
        env_index: usize,
    },
    PathParamForm {
        name: String,
        value: EditorState,
        index: usize,
    },
    /// Sent-request history, newest first.
    History {
        selected: usize,
    },
    CollectionFinder {
        query: EditorState,
        selected: usize,
        /// false = typing in the search input; true = navigating the list (vim keys).
        browse: bool,
        /// Entry awaiting a y/n delete confirmation.
        confirm_delete: Option<SidebarEntry>,
    },
}

const SCRIPT_CONSOLE_LIMIT: usize = 50;

pub fn single_line_state(text: &str) -> EditorState {
    let mut state = EditorState::new(Lines::from(text));
    state.set_single_line(true);
    state.mode = EditorMode::Insert;
    state
}

/// Single-line vim input that starts in Normal mode (press `i`/`a` to type).
pub fn vim_line_state(text: &str) -> EditorState {
    let mut state = EditorState::new(Lines::from(text));
    state.set_single_line(true);
    state.mode = EditorMode::Normal;
    state
}

#[derive(Clone)]
pub struct RequestDraft {
    pub item: RequestItem,
    pub response: Option<ResponseData>,
    pub last_error: Option<String>,
    /// The request's vim buffers, kept so undo/redo survives tab switches (and restarts).
    pub buffers: EditorBuffers,
}

const HISTORY_LIMIT: usize = 500;
/// Autosave drafts this long after the last change…
const SESSION_SAVE_IDLE: Duration = Duration::from_secs(1);
/// …or at the latest this long after the first unsaved change, even while typing.
const SESSION_SAVE_MAX_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestSource {
    pub collection_index: usize,
    pub request_index: usize,
}

pub struct App {
    pub workspace: Workspace,
    pub expanded: HashSet<Uuid>,
    pub drafts: HashMap<Uuid, RequestDraft>,

    pub leader: LeaderState,

    pub current: RequestItem,
    pub source: Option<RequestSource>,

    pub focus: Focus,
    /// Open-tabs sidebar (toggled with `space t`).
    pub sidebar_visible: bool,
    /// Where `space t` returns to when it hides the sidebar.
    pub focus_before_tabs: Focus,
    pub tab_index: usize,
    /// Tab awaiting a y/n "discard unsaved changes" confirmation.
    pub tab_confirm_close: Option<Uuid>,
    pub panel_mode: PanelMode,
    pub maximized: bool,
    pub editor_events: EditorEventHandler,

    pub url_state: EditorState,
    pub body_state: EditorState,

    pub script_tab: ScriptTab,
    pub pre_script_state: EditorState,
    pub post_script_state: EditorState,
    pub script_console: Vec<String>,
    /// Read-only vim view over `script_console`, rebuilt whenever new lines arrive.
    pub console_state: EditorState,

    pub header_index: usize,
    pub form_field_index: usize,
    pub path_param_index: usize,
    pub response: Option<ResponseData>,
    pub last_error: Option<String>,

    pub response_state: EditorState,
    pub response_is_json: bool,

    pub loading: bool,
    pub status_message: String,
    pub history: Vec<HistoryEntry>,

    pub env_index: usize,
    pub env_var_index: usize,

    pub popup: Popup,

    pub should_quit: bool,

    /// (first, last) change since drafts.json was last written.
    pub session_changes: Option<(Instant, Instant)>,
}

impl App {
    pub fn new(workspace: Workspace) -> Self {
        let mut expanded = HashSet::new();
        for c in &workspace.collections {
            expanded.insert(c.id);
        }
        let mut url_state = EditorState::default();
        url_state.set_single_line(true);
        let body_state = EditorState::default();

        Self {
            workspace,
            expanded,
            drafts: HashMap::new(),
            leader: LeaderState::default(),
            current: RequestItem::new("untitled"),
            source: None,
            focus: Focus::Method,
            sidebar_visible: false,
            focus_before_tabs: Focus::Method,
            tab_index: 0,
            tab_confirm_close: None,
            panel_mode: PanelMode::default(),
            maximized: false,
            editor_events: EditorEventHandler::default(),
            url_state,
            body_state,
            script_tab: ScriptTab::default(),
            pre_script_state: EditorState::default(),
            post_script_state: EditorState::default(),
            script_console: Vec::new(),
            console_state: EditorState::default(),
            header_index: 0,
            form_field_index: 0,
            path_param_index: 0,
            response: None,
            last_error: None,
            response_state: EditorState::default(),
            response_is_json: false,
            loading: false,
            status_message: String::from(
                "hjkl: select pane | Enter: focus, Esc: unfocus | space f: find request, space u/m/p/h/b/s/c/r: jump to pane | Ctrl+Enter: send | e: environments | q: quit",
            ),
            history: Vec::new(),
            env_index: 0,
            env_var_index: 0,
            popup: Popup::None,
            should_quit: false,
            session_changes: None,
        }
    }

    pub fn move_focus(&mut self, dir: Direction) {
        if let Some(next) = self.focus.neighbor(dir) {
            if next == Focus::Tabs && !self.sidebar_visible {
                return;
            }
            self.focus = next;
        }
    }

    pub fn flatten_sidebar(&self) -> Vec<SidebarEntry> {
        let mut entries = Vec::new();
        for (ci, c) in self.workspace.collections.iter().enumerate() {
            entries.push(SidebarEntry::Collection(ci));
            if self.expanded.contains(&c.id) {
                for (ri, _) in c.requests.iter().enumerate() {
                    entries.push(SidebarEntry::Request(ci, ri));
                }
            }
        }
        entries
    }

    pub fn collection_finder_matches(&self, query: &str, browse: bool) -> Vec<SidebarEntry> {
        if query.trim().is_empty() {
            // Browsing shows the full collection tree; the bare finder shows Recent first.
            let recent = self.recent_entries();
            if !browse && !recent.is_empty() {
                return recent;
            }
            return self.flatten_sidebar();
        }
        self.fuzzy_ranked_requests(query)
    }

    /// Recently opened requests, newest first. Ids of deleted requests are skipped.
    pub fn recent_entries(&self) -> Vec<SidebarEntry> {
        self.workspace
            .recent_requests
            .iter()
            .filter_map(|id| {
                self.workspace.collections.iter().enumerate().find_map(|(ci, c)| {
                    c.requests
                        .iter()
                        .position(|r| r.id == *id)
                        .map(|ri| SidebarEntry::Request(ci, ri))
                })
            })
            .collect()
    }

    fn record_recent(&mut self, id: Uuid) {
        const RECENT_LIMIT: usize = 20;
        let recent = &mut self.workspace.recent_requests;
        recent.retain(|r| *r != id);
        recent.insert(0, id);
        recent.truncate(RECENT_LIMIT);
    }

    fn fuzzy_ranked_requests(&self, query: &str) -> Vec<SidebarEntry> {
        let matcher = SkimMatcherV2::default();
        let mut scored: Vec<(i64, SidebarEntry)> = self
            .workspace
            .collections
            .iter()
            .enumerate()
            .flat_map(|(ci, c)| c.requests.iter().enumerate().map(move |(ri, r)| (ci, ri, c, r)))
            .filter_map(|(ci, ri, c, r)| {
                let label = format!("{} / {} {}", c.name, r.method.as_str(), r.name);
                matcher
                    .fuzzy_match(&label, query)
                    .map(|score| (score, SidebarEntry::Request(ci, ri)))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, entry)| entry).collect()
    }

    pub fn active_environment(&self) -> Option<&Environment> {
        self.workspace
            .active_environment
            .and_then(|id| self.workspace.environments.iter().find(|e| e.id == id))
    }

    pub fn replace_environment_matching_id(&mut self, env: Option<Environment>) {
        let Some(env) = env else { return };
        if let Some(existing) = self.workspace.environments.iter_mut().find(|e| e.id == env.id) {
            *existing = env;
        }
    }

    pub fn sync_current_from_editors(&mut self) {
        self.current.url = self.url_state.lines.to_string();
        self.current.body = self.body_state.lines.to_string();
        self.current.pre_request_script = self.pre_script_state.lines.to_string();
        self.current.post_response_script = self.post_script_state.lines.to_string();
        self.current.sync_path_params();
        if self.path_param_index >= self.current.path_params.len() {
            self.path_param_index = self.current.path_params.len().saturating_sub(1);
        }
    }

    pub fn sync_editors_from_current(&mut self) {
        self.url_state = EditorState::new(Lines::from(self.current.url.as_str()));
        self.url_state.set_single_line(true);
        self.body_state = EditorState::new(Lines::from(self.current.body.as_str()));
        self.pre_script_state = EditorState::new(Lines::from(self.current.pre_request_script.as_str()));
        self.post_script_state = EditorState::new(Lines::from(self.current.post_response_script.as_str()));
    }

    pub fn push_script_log(&mut self, lines: Vec<String>) {
        self.script_console.extend(lines);
        self.drop_oldest_script_console_lines_past_limit();
        self.rebuild_console_state();
    }

    /// Fresh buffer with the cursor on the last line, so the newest output is in view.
    fn rebuild_console_state(&mut self) {
        self.console_state = EditorState::new(Lines::from(self.script_console.join("\n").as_str()));
        let last_row = self.script_console.len().saturating_sub(1);
        self.console_state.cursor = edtui::Index2::new(last_row, 0);
    }

    fn drop_oldest_script_console_lines_past_limit(&mut self) {
        let excess = self.script_console.len().saturating_sub(SCRIPT_CONSOLE_LIMIT);
        self.script_console.drain(0..excess);
    }

    pub fn find_request(&self, id: Uuid) -> Option<(usize, usize)> {
        self.workspace.collections.iter().enumerate().find_map(|(ci, c)| {
            c.requests.iter().position(|r| r.id == id).map(|ri| (ci, ri))
        })
    }

    /// Drop tabs whose request no longer exists (e.g. deleted in the browser).
    pub fn prune_tabs(&mut self) {
        let alive: HashSet<Uuid> = self
            .workspace
            .collections
            .iter()
            .flat_map(|c| c.requests.iter().map(|r| r.id))
            .collect();
        self.workspace.open_tabs.retain(|id| alive.contains(id));
        self.tab_index = self.tab_index.min(self.workspace.open_tabs.len().saturating_sub(1));
    }

    /// Current in-memory version of a request: the live editors for the loaded one,
    /// its stashed draft otherwise, or None when it has never been edited this session.
    fn working_copy(&self, id: Uuid) -> Option<RequestItem> {
        if self.source.is_some() && self.current.id == id {
            let mut item = self.current.clone();
            item.url = self.url_state.lines.to_string();
            item.body = self.body_state.lines.to_string();
            item.pre_request_script = self.pre_script_state.lines.to_string();
            item.post_response_script = self.post_script_state.lines.to_string();
            item.sync_path_params();
            return Some(item);
        }
        self.drafts.get(&id).map(|d| d.item.clone())
    }

    /// True when the request has edits that `space w` hasn't written back yet.
    pub fn is_dirty(&self, id: Uuid) -> bool {
        let Some(working) = self.working_copy(id) else {
            return false;
        };
        let Some((ci, ri)) = self.find_request(id) else {
            return false;
        };
        let mut saved = self.workspace.collections[ci].requests[ri].clone();
        // Loading fills path_params from the URL; don't count that as an edit.
        saved.sync_path_params();
        serde_json::to_value(&saved).ok() != serde_json::to_value(&working).ok()
    }

    /// Close a tab, discarding its unsaved draft. Closing the loaded request
    /// switches to a neighbouring tab, or clears the editor when none is left.
    pub fn close_tab(&mut self, id: Uuid) {
        let Some(pos) = self.workspace.open_tabs.iter().position(|t| *t == id) else {
            return;
        };
        self.workspace.open_tabs.remove(pos);
        self.drafts.remove(&id);
        let closing_current = self.source.is_some() && self.current.id == id;
        if closing_current {
            // Don't let load_request stash the tab we're discarding.
            self.source = None;
            let next = self
                .workspace
                .open_tabs
                .get(pos)
                .or_else(|| self.workspace.open_tabs.get(pos.saturating_sub(1)))
                .copied();
            match next.and_then(|n| self.find_request(n)) {
                Some((ci, ri)) => self.load_request(ci, ri),
                None => {
                    self.restore_saved_item(RequestItem::new("untitled"));
                    self.sync_editors_from_current();
                    self.refresh_response_view();
                }
            }
        }
        self.tab_index = pos.min(self.workspace.open_tabs.len().saturating_sub(1));
        self.persist();
    }

    pub fn load_request(&mut self, coll_idx: usize, req_idx: usize) {
        self.stash_current_draft();

        let Some(item) = self
            .workspace
            .collections
            .get(coll_idx)
            .and_then(|c| c.requests.get(req_idx))
            .cloned()
        else {
            return;
        };

        match self.drafts.get(&item.id).cloned() {
            Some(draft) => self.restore_draft(draft),
            None => {
                self.restore_saved_item(item);
                self.sync_editors_from_current();
            }
        }
        self.current.sync_path_params();
        self.source = Some(RequestSource { collection_index: coll_idx, request_index: req_idx });
        self.header_index = 0;
        self.form_field_index = 0;
        self.path_param_index = 0;
        self.refresh_response_view();
        self.record_recent(self.current.id);
        if !self.workspace.open_tabs.contains(&self.current.id) {
            self.workspace.open_tabs.push(self.current.id);
        }
        self.persist();
        self.status_message = format!("Loaded '{}'", self.current.name);
    }

    fn restore_draft(&mut self, draft: RequestDraft) {
        self.current = draft.item;
        self.response = draft.response;
        self.last_error = draft.last_error;
        self.url_state = draft.buffers.url;
        self.body_state = draft.buffers.body;
        self.pre_script_state = draft.buffers.pre;
        self.post_script_state = draft.buffers.post;
    }

    fn live_buffers(&self) -> EditorBuffers {
        EditorBuffers {
            url: self.url_state.clone(),
            body: self.body_state.clone(),
            pre: self.pre_script_state.clone(),
            post: self.post_script_state.clone(),
        }
    }

    fn restore_saved_item(&mut self, item: RequestItem) {
        self.current = item;
        self.response = None;
        self.last_error = None;
    }

    fn stash_current_draft(&mut self) {
        if self.source.is_none() {
            return;
        }
        self.sync_current_from_editors();
        self.drafts.insert(
            self.current.id,
            RequestDraft {
                item: self.current.clone(),
                response: self.response.clone(),
                last_error: self.last_error.clone(),
                buffers: self.live_buffers(),
            },
        );
    }

    /// Replace the stored copy of `item` (matched by id) with it. Returns false when
    /// the request no longer exists in any collection.
    fn write_back(&mut self, item: &RequestItem) -> bool {
        let Some((ci, ri)) = self.find_request(item.id) else {
            return false;
        };
        self.workspace.collections[ci].requests[ri] = item.clone();
        true
    }

    /// Save every request with unsaved edits (the loaded one and all drafts).
    pub fn save_all(&mut self) {
        self.sync_current_from_editors();
        let mut dirty: Vec<RequestItem> = Vec::new();
        if self.source.is_some() && self.is_dirty(self.current.id) {
            dirty.push(self.current.clone());
        }
        for (id, draft) in &self.drafts {
            let is_current = self.source.is_some() && *id == self.current.id;
            if !is_current && self.is_dirty(*id) {
                dirty.push(draft.item.clone());
            }
        }
        let saved = dirty.iter().filter(|item| self.write_back(item)).count();
        if saved == 0 {
            self.status_message = "Nothing to save".into();
            return;
        }
        self.persist();
        self.mark_session_changed();
        if !self.status_message.starts_with("Failed to save") {
            self.status_message = format!("Saved {saved} request(s)");
        }
    }

    pub fn save_current(&mut self) {
        self.sync_current_from_editors();
        let current = self.current.clone();
        if self.source.is_none() || !self.write_back(&current) {
            self.status_message =
                "No collection request loaded — open <space> f and press a (or Ctrl+R) to create one first".into();
            return;
        }
        let name = current.name;
        self.persist();
        self.mark_session_changed();
        if !self.status_message.starts_with("Failed to save") {
            self.status_message = format!("Saved '{name}'");
        }
    }

    pub fn selected_header_mut(&mut self) -> Option<&mut Header> {
        self.current.headers.get_mut(self.header_index)
    }

    pub fn selected_form_field_mut(&mut self) -> Option<&mut FormField> {
        self.current.form_fields.get_mut(self.form_field_index)
    }

    pub fn refresh_response_view(&mut self) {
        let text = match (&self.response, &self.last_error) {
            (Some(resp), _) => {
                let mut out = String::new();
                for (k, v) in &resp.headers {
                    out.push_str(k);
                    out.push_str(": ");
                    out.push_str(v);
                    out.push('\n');
                }
                out.push('\n');
                let pretty = crate::format::pretty_json(&resp.body);
                self.response_is_json = pretty.is_some();
                out.push_str(&pretty.unwrap_or_else(|| resp.body.clone()));
                out
            }
            (None, Some(err)) => {
                self.response_is_json = false;
                let mut out = String::from("Request failed:\n");
                for cause in err.split(": ") {
                    out.push_str("  ");
                    out.push_str(cause);
                    out.push('\n');
                }
                out
            }
            (None, None) => {
                self.response_is_json = false;
                "No response yet. Focus here after sending a request (F5 / Ctrl+Enter).".into()
            }
        };
        self.response_state = EditorState::new(Lines::from(text));
    }

    pub fn persist(&mut self) {
        if let Err(e) = crate::storage::save(&self.workspace) {
            let detail = format!("{e:#}");
            crate::logger::log("workspace save failed", &detail);
            self.status_message = format!(
                "Failed to save workspace: {detail} (see {})",
                crate::logger::path_hint()
            );
        }
    }

    pub fn push_history(&mut self, status: Option<u16>, duration_ms: Option<u128>) {
        self.history.push(HistoryEntry {
            timestamp: chrono::Local::now(),
            request_id: self.source.map(|_| self.current.id),
            name: self.current.name.clone(),
            method: self.current.method,
            url: self.current.url.clone(),
            status,
            duration_ms,
        });
        let excess = self.history.len().saturating_sub(HISTORY_LIMIT);
        self.history.drain(0..excess);
        if let Err(e) = crate::storage::save_history(&self.history) {
            crate::logger::log("history save failed", &format!("{e:#}"));
        }
    }

    pub fn mark_session_changed(&mut self) {
        let now = Instant::now();
        self.session_changes = Some(match self.session_changes {
            Some((first, _)) => (first, now),
            None => (now, now),
        });
    }

    pub fn session_save_due(&self) -> bool {
        self.session_changes.is_some_and(|(first, last)| {
            last.elapsed() >= SESSION_SAVE_IDLE || first.elapsed() >= SESSION_SAVE_MAX_DELAY
        })
    }

    /// Requests worth keeping across a restart: unsaved edits, a response/error, or undo history.
    fn session_snapshot(&self) -> StoredSession {
        let mut drafts = HashMap::new();
        let mut keep = |id: Uuid, item: &RequestItem, resp: &Option<ResponseData>, err: &Option<String>, buffers: &EditorBuffers| {
            let history = buffers.to_stored();
            if self.is_dirty(id) || resp.is_some() || err.is_some() || !history.is_empty() {
                drafts.insert(
                    id,
                    StoredDraft { item: item.clone(), response: resp.clone(), last_error: err.clone(), history },
                );
            }
        };
        let current_loaded = self.source.is_some();
        for (id, d) in &self.drafts {
            if current_loaded && *id == self.current.id {
                continue;
            }
            keep(*id, &d.item, &d.response, &d.last_error, &d.buffers);
        }
        if current_loaded {
            let mut item = self.current.clone();
            item.url = self.url_state.lines.to_string();
            item.body = self.body_state.lines.to_string();
            item.pre_request_script = self.pre_script_state.lines.to_string();
            item.post_response_script = self.post_script_state.lines.to_string();
            item.sync_path_params();
            keep(self.current.id, &item, &self.response, &self.last_error, &self.live_buffers());
        }
        StoredSession { drafts }
    }

    pub fn save_session(&mut self) {
        self.session_changes = None;
        if let Err(e) = crate::storage::save_session(&self.session_snapshot()) {
            let detail = format!("{e:#}");
            crate::logger::log("drafts save failed", &detail);
            self.status_message = format!("Failed to save drafts: {detail}");
        }
    }

    /// Load drafts from the last run and reopen the most recently used open tab.
    pub fn restore_session(&mut self, session: StoredSession) {
        for (id, d) in session.drafts {
            if self.find_request(id).is_none() {
                continue;
            }
            let buffers = EditorBuffers::from_stored(&d.item, &d.history);
            self.drafts.insert(
                id,
                RequestDraft { item: d.item, response: d.response, last_error: d.last_error, buffers },
            );
        }
        let last = self
            .workspace
            .recent_requests
            .iter()
            .find(|id| self.workspace.open_tabs.contains(id))
            .copied();
        if let Some((ci, ri)) = last.and_then(|id| self.find_request(id)) {
            self.load_request(ci, ri);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Collection;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
        let state = &mut app.body_state;
        app.editor_events.on_key_event(KeyEvent::new(code, mods), state);
    }

    /// Everything that touches disk goes to a throwaway HOME, never the real workspace.
    #[test]
    fn unsaved_edits_and_undo_survive_tab_switch_and_restart() {
        let home = std::env::temp_dir().join(format!("postui-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        // SAFETY: the only test in this binary that reads HOME / touches storage.
        unsafe { std::env::set_var("HOME", &home) };
        assert!(dirs::data_dir().unwrap().starts_with(&home));

        let mut ws = Workspace::default();
        let mut c = Collection::new("c");
        let mut a = RequestItem::new("a");
        a.body = "x".into();
        c.requests.push(a);
        c.requests.push(RequestItem::new("b"));
        let (a_id, b_id) = (c.requests[0].id, c.requests[1].id);
        ws.collections.push(c);

        let mut app = App::new(ws.clone());
        app.load_request(0, 0);
        press(&mut app, KeyCode::Char('A'), KeyModifiers::SHIFT);
        for ch in "yz".chars() {
            press(&mut app, KeyCode::Char(ch), KeyModifiers::NONE);
        }
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.is_dirty(a_id));

        // Switch tabs and back: edits and undo history are kept.
        app.load_request(0, 1);
        assert!(app.is_dirty(a_id) && !app.is_dirty(b_id));
        assert_eq!(app.workspace.open_tabs, vec![a_id, b_id]);

        // "Restart": persist, then build a fresh App from what's on disk.
        app.save_session();
        let mut app2 = App::new(crate::storage::load().unwrap());
        app2.restore_session(crate::storage::load_session());
        assert_eq!(app2.current.id, b_id, "reopens the most recent open tab");
        assert!(app2.is_dirty(a_id));
        app2.load_request(0, 0);
        assert_eq!(app2.body_state.lines.to_string(), "xyz");
        press(&mut app2, KeyCode::Char('u'), KeyModifiers::NONE);
        assert_eq!(app2.body_state.lines.to_string(), "x", "undo history restored");
        press(&mut app2, KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(app2.body_state.lines.to_string(), "xyz", "redo works too");

        // space W writes every dirty request back.
        app2.save_all();
        assert!(!app2.is_dirty(a_id));
        assert_eq!(app2.workspace.collections[0].requests[0].body, "xyz");

        let _ = std::fs::remove_dir_all(&home);
    }
}
