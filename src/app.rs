use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use edtui::{EditorEventHandler, EditorMode, EditorState, Lines};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use uuid::Uuid;

use crate::completion::VarMenu;
use crate::model::{
    BodyMode, Environment, FormField, Header, HistoryEntry, RequestItem, ResponseData,
    ResponseMeta, ResponseRecord, Workspace,
};
use crate::session::{EditorBuffers, StoredBuffers, StoredDraft, StoredSession};

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

// Only one popup exists at a time, so the size difference between variants doesn't matter.
#[allow(clippy::large_enum_variant)]
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
    /// Save the response body to a file (`S` in the Response pane).
    SaveResponse {
        path: EditorState,
        completion: crate::save::PathCompletion,
        /// The chosen file exists: waiting for `o` (overwrite), `n` (new name) or Esc.
        conflict: Option<std::path::PathBuf>,
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

/// What happened when saving a response body to a file.
#[derive(Debug, PartialEq, Eq)]
pub enum SaveResult {
    Saved,
    /// The file exists and overwriting wasn't confirmed.
    Exists(std::path::PathBuf),
    /// Nothing written; the status line says why.
    Failed,
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
const RESPONSES_PER_REQUEST: usize = 30;
const RESPONSES_TOTAL: usize = 1000;
const RESPONSE_BODY_LIMIT: usize = 2 * 1024 * 1024;
/// Bodies larger than this are shown in "light mode": only this much, raw, without
/// highlighting or undo. The full body is a keypress away (`o` pager, `S` save).
pub const LIGHT_MODE_BYTES: usize = 1024 * 1024;
/// Longest line the Response pane gets. edtui's rendering cost grows faster than
/// linearly with line length (a 1M-char line takes seconds per frame), so longer
/// lines (minified JSON/HTML, base64 fields) are split for display.
const VIEW_MAX_LINE_CHARS: usize = 1000;
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
    /// `m` pressed in the tabs sidebar; the next digit assigns a quick slot.
    pub tab_mark_pending: bool,
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
    /// Past responses per request, oldest first. Summaries only; bodies stay on disk.
    pub response_history: HashMap<Uuid, Vec<ResponseMeta>>,
    /// Which past response the Response pane shows; None = the latest.
    pub response_view: Option<usize>,
    /// The past response being viewed, read from disk when stepping with `[` / `]`.
    pub viewed_response: Option<ResponseRecord>,
    /// Full body size when the response on screen is shown in light mode (truncated).
    pub response_light: Option<usize>,
    /// Some lines of the response on screen were split for display.
    pub response_wrapped: bool,
    /// A file the main loop should open in `$PAGER` (set by `o` in the Response pane).
    pub pending_pager: Option<std::path::PathBuf>,

    /// Requests with a send in flight (count, in case one is sent twice).
    pub in_flight: HashMap<Uuid, usize>,
    pub status_message: String,
    pub history: Vec<HistoryEntry>,

    pub env_index: usize,
    pub env_var_index: usize,

    pub popup: Popup,

    pub should_quit: bool,

    /// (first, last) change since drafts.json was last written.
    pub session_changes: Option<(Instant, Instant)>,

    /// `{{variable}}` completion menu, when open (see `completion`).
    pub var_menu: Option<VarMenu>,

    /// Hash of the drafts.json content last written, to skip unchanged autosaves.
    last_session_hash: Option<u64>,
    /// workspace.json has changes (recent requests, open tabs) waiting for the next
    /// autosave; see `persist_later`.
    workspace_dirty: bool,
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
            tab_mark_pending: false,
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
            response_history: HashMap::new(),
            response_view: None,
            viewed_response: None,
            response_light: None,
            response_wrapped: false,
            pending_pager: None,
            in_flight: HashMap::new(),
            status_message: String::from(
                "hjkl: select pane | Enter: focus, Esc: unfocus | space f: find request, space u/m/p/h/b/s/c/r: jump to pane | Ctrl+Enter: send | space e: environments | q: quit",
            ),
            history: Vec::new(),
            env_index: 0,
            env_var_index: 0,
            popup: Popup::None,
            should_quit: false,
            session_changes: None,
            var_menu: None,
            last_session_hash: None,
            workspace_dirty: false,
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
                self.workspace
                    .collections
                    .iter()
                    .enumerate()
                    .find_map(|(ci, c)| {
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
            .flat_map(|(ci, c)| {
                c.requests
                    .iter()
                    .enumerate()
                    .map(move |(ri, r)| (ci, ri, c, r))
            })
            .filter_map(|(ci, ri, c, r)| {
                let label = format!("{} / {} {}", c.name, r.method.as_str(), r.name);
                matcher
                    .fuzzy_match(&label, query)
                    .map(|score| (score, SidebarEntry::Request(ci, ri)))
            })
            .collect();
        scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        scored.into_iter().map(|(_, entry)| entry).collect()
    }

    /// The focused vim input whose text gets `{{variable}}` substitution, if any —
    /// the inputs that offer variable completion.
    pub fn var_input(&mut self) -> Option<&mut EditorState> {
        match &mut self.popup {
            Popup::HeaderForm {
                key,
                value,
                field,
                interacting: true,
                ..
            } => Some(match field {
                HeaderFormField::Key => key,
                HeaderFormField::Value => value,
            }),
            Popup::PathParamForm { value, .. } => Some(value),
            Popup::None if self.panel_mode == PanelMode::Interact => match self.focus {
                Focus::Url => Some(&mut self.url_state),
                Focus::Body if self.current.body_mode == BodyMode::Raw => {
                    Some(&mut self.body_state)
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// (name, value) pairs of the active environment; empty without one.
    pub fn active_env_vars(&self) -> Vec<(String, String)> {
        self.active_environment()
            .map(|e| e.variables.clone())
            .unwrap_or_default()
    }

    pub fn active_environment(&self) -> Option<&Environment> {
        self.workspace
            .active_environment
            .and_then(|id| self.workspace.environments.iter().find(|e| e.id == id))
    }

    pub fn replace_environment_matching_id(&mut self, env: Option<Environment>) {
        let Some(env) = env else { return };
        if let Some(existing) = self
            .workspace
            .environments
            .iter_mut()
            .find(|e| e.id == env.id)
        {
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
        self.pre_script_state =
            EditorState::new(Lines::from(self.current.pre_request_script.as_str()));
        self.post_script_state =
            EditorState::new(Lines::from(self.current.post_response_script.as_str()));
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
        let excess = self
            .script_console
            .len()
            .saturating_sub(SCRIPT_CONSOLE_LIMIT);
        self.script_console.drain(0..excess);
    }

    pub fn find_request(&self, id: Uuid) -> Option<(usize, usize)> {
        self.workspace
            .collections
            .iter()
            .enumerate()
            .find_map(|(ci, c)| {
                c.requests
                    .iter()
                    .position(|r| r.id == id)
                    .map(|ri| (ci, ri))
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
        self.workspace
            .quick_slots
            .retain(|_, id| alive.contains(id));
        self.tab_index = self
            .tab_index
            .min(self.workspace.open_tabs.len().saturating_sub(1));
    }

    /// The quick slot (1-9) a request is assigned to, if any.
    pub fn slot_of(&self, id: Uuid) -> Option<u8> {
        self.workspace
            .quick_slots
            .iter()
            .find(|(_, v)| **v == id)
            .map(|(k, _)| *k)
    }

    /// Put a request in a quick slot. A request holds at most one slot, and a slot
    /// one request: the request's old slot is freed, and the slot's old holder loses it.
    pub fn assign_slot(&mut self, id: Uuid, slot: u8) {
        let name = |app: &App, id: Uuid| {
            app.find_request(id)
                .map(|(ci, ri)| app.workspace.collections[ci].requests[ri].name.clone())
        };
        self.workspace.quick_slots.retain(|_, v| *v != id);
        let replaced = self
            .workspace
            .quick_slots
            .insert(slot, id)
            .and_then(|old| name(self, old));
        self.persist();
        let this = name(self, id).unwrap_or_default();
        self.status_message = match replaced {
            Some(old) => format!("space {slot} → '{this}' (was '{old}')"),
            None => format!("space {slot} → '{this}'"),
        };
    }

    pub fn clear_slot_of(&mut self, id: Uuid) {
        if let Some(slot) = self.slot_of(id) {
            self.workspace.quick_slots.remove(&slot);
            self.persist();
            self.status_message = format!("space {slot} cleared");
        }
    }

    /// `space <digit>`: open the request in that slot.
    pub fn open_slot(&mut self, slot: u8) {
        let target = self
            .workspace
            .quick_slots
            .get(&slot)
            .and_then(|id| self.find_request(*id));
        match target {
            Some((ci, ri)) => self.load_request(ci, ri),
            None => {
                self.status_message = format!(
                    "space {slot} is empty — in the tabs sidebar (space t), select a tab and press m {slot}"
                )
            }
        }
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
        if let Some(draft) = self.drafts.remove(&id) {
            discard_body_file(draft.response.as_ref());
        }
        let closing_current = self.source.is_some() && self.current.id == id;
        if closing_current {
            discard_body_file(self.response.take().as_ref());
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
        self.persist_later();
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

        // Move the draft out rather than cloning it (it can hold a large response and
        // undo history); it's stashed again when this request is left.
        match self.drafts.remove(&item.id) {
            Some(draft) => self.restore_draft(draft),
            None => {
                self.restore_saved_item(item);
                self.sync_editors_from_current();
            }
        }
        self.current.sync_path_params();
        self.source = Some(RequestSource {
            collection_index: coll_idx,
            request_index: req_idx,
        });
        self.response_view = None;
        self.viewed_response = None;
        if self.response.is_none() && self.last_error.is_none() {
            self.load_latest_response_from_history();
        }
        self.header_index = 0;
        self.form_field_index = 0;
        self.path_param_index = 0;
        self.refresh_response_view();
        self.record_recent(self.current.id);
        if !self.workspace.open_tabs.contains(&self.current.id) {
            self.workspace.open_tabs.push(self.current.id);
        }
        self.persist_later();
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
        let (response, last_error) = match (&self.response_view, &self.viewed_response) {
            (Some(_), Some(past)) => (&past.response, &past.error),
            _ => (&self.response, &self.last_error),
        };
        self.response_light = None;
        self.response_wrapped = false;
        let text = match (response, last_error) {
            (Some(resp), _) => {
                let mut out = String::new();
                for (k, v) in &resp.headers {
                    out.push_str(k);
                    out.push_str(": ");
                    out.push_str(v);
                    out.push('\n');
                }
                out.push('\n');
                let body = if resp.binary {
                    self.response_is_json = false;
                    let kind = resp.header("content-type").unwrap_or("no content type");
                    let action = if resp.body_file.is_some() {
                        "S: save to file"
                    } else {
                        "past binary bodies aren't kept; send again to save it"
                    };
                    format!(
                        "── Binary body · {} · {kind} — {action} ──",
                        human_size(resp.size as usize)
                    )
                } else if resp.body.len() > LIGHT_MODE_BYTES {
                    // Light mode: a fully pretty-printed, highlighted multi-MB buffer costs
                    // ~20x the body in memory, so show only the start and offer the rest.
                    self.response_is_json = false;
                    self.response_light = Some(resp.body.len());
                    if looks_like_json(&resp.body) {
                        crate::format::pretty_json_prefix(&resp.body, LIGHT_MODE_BYTES)
                    } else {
                        let cut = floor_char_boundary(&resp.body, LIGHT_MODE_BYTES);
                        resp.body[..cut].to_string()
                    }
                } else {
                    let pretty = crate::format::pretty_json(&resp.body);
                    self.response_is_json = pretty.is_some();
                    pretty.unwrap_or_else(|| resp.body.clone())
                };
                let (body, wrapped) = split_long_lines(body, VIEW_MAX_LINE_CHARS);
                self.response_wrapped = wrapped;
                out.push_str(&body);
                if let Some(total) = self.response_light {
                    out.push_str(&format!(
                        "\n\n── light mode: showing the start of {} — o: open all in $PAGER, S: save to file ──\n",
                        human_size(total)
                    ));
                }
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

    /// Write workspace.json now. Use for changes the user would be upset to lose
    /// (saves, creating/deleting, environments, quick slots).
    pub fn persist(&mut self) {
        self.workspace_dirty = false; // this write includes any pending changes
        if let Err(e) = crate::storage::save(&self.workspace) {
            let detail = format!("{e:#}");
            crate::logger::log("workspace save failed", &detail);
            self.status_message = format!(
                "Failed to save workspace: {detail} (see {})",
                crate::logger::path_hint()
            );
        }
    }

    pub fn push_history(
        &mut self,
        sent: &RequestItem,
        status: Option<u16>,
        duration_ms: Option<u128>,
    ) {
        self.history.push(HistoryEntry {
            timestamp: chrono::Local::now(),
            request_id: self.find_request(sent.id).map(|_| sent.id),
            name: sent.name.clone(),
            method: sent.method,
            url: sent.url.clone(),
            status,
            duration_ms,
        });
        let excess = self.history.len().saturating_sub(HISTORY_LIMIT);
        self.history.drain(0..excess);
        if let Err(e) = crate::storage::save_history(&self.history) {
            crate::logger::log("history save failed", &format!("{e:#}"));
        }
    }

    /// Index responses.jsonl (summaries only; bodies stay on disk), dropping records
    /// of deleted requests, unreadable lines, and anything past the per-request / total
    /// limits. The file is rewritten only when something was dropped.
    pub fn load_response_history(&mut self, lines: Vec<String>) {
        let parsed: Vec<_> = lines
            .iter()
            .map(|l| crate::storage::summarize_response_line(l))
            .collect();
        // Keep the newest N per request and overall, walking from the end.
        let mut per_request: HashMap<Uuid, usize> = HashMap::new();
        let mut keep = vec![false; lines.len()];
        let mut total = 0;
        for (i, entry) in parsed.iter().enumerate().rev() {
            let Some((id, _)) = entry else { continue };
            if total == RESPONSES_TOTAL || self.find_request(*id).is_none() {
                continue;
            }
            let n = per_request.entry(*id).or_default();
            if *n < RESPONSES_PER_REQUEST {
                *n += 1;
                total += 1;
                keep[i] = true;
            }
        }
        if keep.iter().any(|k| !k) {
            let kept: Vec<&str> = lines
                .iter()
                .zip(&keep)
                .filter(|(_, k)| **k)
                .map(|(l, _)| l.as_str())
                .collect();
            if let Err(e) = crate::storage::rewrite_response_lines(&kept) {
                crate::logger::log("responses compact failed", &format!("{e:#}"));
            }
        }
        // Offsets of the kept lines in the (possibly rewritten) file.
        self.response_history.clear();
        let mut offset = 0u64;
        for ((line, entry), kept) in lines.iter().zip(parsed).zip(keep) {
            if !kept {
                continue;
            }
            if let Some((id, mut meta)) = entry {
                meta.offset = offset;
                self.response_history.entry(id).or_default().push(meta);
            }
            offset += line.len() as u64 + 1;
        }
    }

    fn record_response(
        &mut self,
        request_id: Uuid,
        response: Option<&ResponseData>,
        error: Option<&str>,
    ) {
        let mut truncated = false;
        let response = response.map(|r| {
            let mut r = r.clone();
            if r.body.len() > RESPONSE_BODY_LIMIT {
                let mut cut = RESPONSE_BODY_LIMIT;
                while !r.body.is_char_boundary(cut) {
                    cut -= 1;
                }
                r.body.truncate(cut);
                truncated = true;
            }
            r
        });
        let record = ResponseRecord {
            request_id,
            timestamp: chrono::Local::now(),
            response,
            error: error.map(str::to_string),
            truncated,
        };
        let offset = match crate::storage::append_response(&record) {
            Ok(offset) => offset,
            Err(e) => {
                crate::logger::log("response history save failed", &format!("{e:#}"));
                return;
            }
        };
        let list = self.response_history.entry(request_id).or_default();
        list.push(ResponseMeta {
            timestamp: record.timestamp,
            status: record.response.as_ref().map(|r| r.status),
            duration_ms: record.response.as_ref().map(|r| r.duration_ms),
            truncated,
            offset,
        });
        let excess = list.len().saturating_sub(RESPONSES_PER_REQUEST);
        list.drain(0..excess);
    }

    /// Show the newest stored response of the loaded request (e.g. after a restart,
    /// when nothing is in memory yet).
    fn load_latest_response_from_history(&mut self) {
        let Some(meta) = self
            .response_history
            .get(&self.current.id)
            .and_then(|l| l.last())
        else {
            return;
        };
        match crate::storage::read_response_at(meta.offset) {
            Ok(record) => {
                self.response = record.response;
                self.last_error = record.error;
            }
            Err(e) => crate::logger::log("response history read failed", &format!("{e:#}")),
        }
    }

    pub fn begin_send(&mut self, id: Uuid) {
        *self.in_flight.entry(id).or_default() += 1;
    }

    fn end_send(&mut self, id: Uuid) {
        if let Some(n) = self.in_flight.get_mut(&id) {
            *n -= 1;
            if *n == 0 {
                self.in_flight.remove(&id);
            }
        }
    }

    /// True while the request on screen has a send in flight.
    pub fn is_sending(&self) -> bool {
        self.in_flight.contains_key(&self.current.id)
    }

    /// A finished send. It lands on the request that was sent — which may no longer
    /// be the one on screen if the user switched tabs while it was in flight.
    pub fn deliver_response(
        &mut self,
        sent: &RequestItem,
        response: Option<ResponseData>,
        error: Option<String>,
    ) {
        self.end_send(sent.id);
        self.record_response(sent.id, response.as_ref(), error.as_deref());
        if self.current.id == sent.id {
            discard_body_file(std::mem::replace(&mut self.response, response).as_ref());
            self.last_error = error;
            self.response_view = None;
            self.viewed_response = None;
            self.refresh_response_view();
            return;
        }
        if !self.drafts.contains_key(&sent.id) {
            let Some((ci, ri)) = self.find_request(sent.id) else {
                return;
            };
            let mut item = self.workspace.collections[ci].requests[ri].clone();
            item.sync_path_params();
            let buffers = EditorBuffers::from_item(&item);
            self.drafts.insert(
                sent.id,
                RequestDraft {
                    item,
                    response: None,
                    last_error: None,
                    buffers,
                },
            );
        }
        if let Some(draft) = self.drafts.get_mut(&sent.id) {
            discard_body_file(std::mem::replace(&mut draft.response, response).as_ref());
            draft.last_error = error;
        }
        self.status_message = format!(
            "Response for '{}' arrived — switch to its tab to see it",
            sent.name
        );
    }

    /// `x` in the Response pane: run the loaded request's post-response script (as it
    /// is in the editor now, unsaved edits included) against the response on screen —
    /// a past one when browsing history with `[` / `]`, otherwise the latest.
    pub fn run_post_script_on_shown_response(&mut self) {
        let Some(resp) = self.shown_response().cloned() else {
            self.status_message =
                "No response on screen to run the post-response script against".into();
            return;
        };
        let script = self.post_script_state.lines.to_string();
        if script.trim().is_empty() {
            self.status_message = "This request has no post-response script".into();
            return;
        }
        let mut env = self.active_environment().cloned();
        let outcome = crate::script::run_post_response(&script, &resp, &mut env);
        let label = match self.response_history_position() {
            Some((pos, total, _)) => format!("response {pos}/{total}"),
            None => "the latest response".to_string(),
        };
        self.push_script_log(
            std::iter::once(format!(
                "— post-response script run manually on {label} ({}) —",
                resp.status
            ))
            .chain(outcome.logs)
            .collect(),
        );
        self.replace_environment_matching_id(env);
        self.status_message = match outcome.error {
            Some(err) => format!("Post-response script failed: {err}"),
            None => format!("Post-response script ran on {label}"),
        };
    }

    /// The response on screen: the past one being viewed, otherwise the latest.
    pub fn shown_response(&self) -> Option<&ResponseData> {
        match self.response_view {
            Some(_) => self.viewed_response.as_ref()?.response.as_ref(),
            None => self.response.as_ref(),
        }
    }

    /// `o` in the Response pane: write the whole body (pretty-printed if it's JSON) to a
    /// temp file and ask the main loop to open it in `$PAGER`.
    pub fn open_shown_response_in_pager(&mut self) {
        let Some(resp) = self.shown_response() else {
            self.status_message = "No response body to open".into();
            return;
        };
        if resp.binary {
            self.status_message = "The body is binary; use S to save it to a file".into();
            return;
        }
        let (text, ext) = match crate::format::pretty_json(&resp.body) {
            Some(pretty) => (pretty, "json"),
            None => (resp.body.clone(), "txt"),
        };
        let path = std::env::temp_dir().join(format!("postui-response-{}.{ext}", Uuid::new_v4()));
        match std::fs::write(&path, text) {
            Ok(()) => self.pending_pager = Some(path),
            Err(e) => {
                self.status_message = format!("Couldn't write a temp file for the pager: {e}")
            }
        }
    }

    /// `S`: open the save popup with the suggested path.
    pub fn open_save_response(&mut self) {
        let Some(resp) = self.shown_response() else {
            self.status_message = "No response body to save".into();
            return;
        };
        if resp.binary && resp.body_file.is_none() {
            self.status_message =
                "This past binary body isn't kept; send the request again to save it".into();
            return;
        }
        let mut path = vim_line_state(&self.suggested_save_path());
        let len = path.lines.len_col(0).unwrap_or(0);
        path.cursor = edtui::Index2::new(0, len.saturating_sub(1));
        self.popup = Popup::SaveResponse {
            path,
            completion: Default::default(),
            conflict: None,
        };
    }

    /// Folder: where this request's body was saved last time, else Downloads (or
    /// home). Name: the server's `Content-Disposition` file name, else the name used
    /// last time, else `<request name>.<extension for the content type>`.
    pub fn suggested_save_path(&self) -> String {
        let last = self
            .workspace
            .response_save_paths
            .get(&self.current.id)
            .map(std::path::PathBuf::from);
        let dir = last
            .as_ref()
            .and_then(|p| p.parent())
            .filter(|d| d.is_dir())
            .map(std::path::Path::to_path_buf)
            .or_else(dirs::download_dir)
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let resp = self.shown_response();
        let from_server = resp
            .and_then(|r| r.header("content-disposition"))
            .and_then(crate::save::filename_from_content_disposition);
        let from_last = last
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        let name = from_server.or(from_last).unwrap_or_else(|| {
            let base = crate::save::sanitize_file_name(&self.current.name)
                .unwrap_or_else(|| "response".into());
            let ext = crate::save::extension_for_content_type(
                resp.and_then(|r| r.header("content-type")),
                resp.is_some_and(|r| r.binary),
            );
            format!("{base}.{ext}")
        });
        let mut path = dir.join(name).display().to_string();
        if let Some(home) = dirs::home_dir().map(|h| h.display().to_string())
            && let Some(rest) = path.strip_prefix(&home)
        {
            path = format!("~{rest}");
        }
        path
    }

    /// Write the body on screen to `input`: a file path, or a folder (ending in `/`
    /// or existing) to put the suggested file name in. An existing file is only
    /// replaced when `overwrite` is set. On success the path is remembered for this
    /// request, to suggest the same folder next time.
    pub fn save_response_to(&mut self, input: &str, overwrite: bool) -> SaveResult {
        let input = input.trim();
        if input.is_empty() {
            self.status_message = "Type a path to save to".into();
            return SaveResult::Failed;
        }
        let mut path = crate::save::expand_tilde(input);
        if input.ends_with('/') || path.is_dir() {
            let suggested = crate::save::expand_tilde(&self.suggested_save_path());
            path = path.join(suggested.file_name().unwrap_or_default());
        }
        if path.exists() && !overwrite {
            return SaveResult::Exists(path);
        }
        let Some(resp) = self.shown_response() else {
            self.status_message = "No response body to save".into();
            return SaveResult::Failed;
        };
        let written = match &resp.body_file {
            Some(file) => std::fs::copy(file, &path).map(|n| n as usize),
            None => std::fs::write(&path, &resp.body).map(|()| resp.body.len()),
        };
        match written {
            Ok(bytes) => {
                self.status_message = format!("Saved {} to {}", human_size(bytes), path.display());
                self.workspace
                    .response_save_paths
                    .insert(self.current.id, path.display().to_string());
                self.persist();
                SaveResult::Saved
            }
            Err(e) => {
                self.status_message = format!("Couldn't save to {}: {e}", path.display());
                SaveResult::Failed
            }
        }
    }

    /// `[` / `]` in the Response pane: step to an older / newer past response.
    pub fn step_response_history(&mut self, older: bool) {
        let n = self
            .response_history
            .get(&self.current.id)
            .map_or(0, Vec::len);
        if n == 0 {
            self.status_message = "No response history for this request yet".into();
            return;
        }
        let pos = self.response_view.unwrap_or(n - 1);
        let next = if older {
            if pos == 0 {
                self.status_message = "Already at the oldest response".into();
                return;
            }
            pos - 1
        } else {
            if pos + 1 >= n {
                self.status_message = "Already at the latest response".into();
                return;
            }
            pos + 1
        };
        if next + 1 == n {
            self.response_view = None;
            self.viewed_response = None;
        } else {
            let offset = self.response_history[&self.current.id][next].offset;
            match crate::storage::read_response_at(offset) {
                Ok(record) => {
                    self.response_view = Some(next);
                    self.viewed_response = Some(record);
                }
                Err(e) => {
                    self.status_message = format!("Couldn't read that response: {e:#}");
                    return;
                }
            }
        }
        self.refresh_response_view();
    }

    /// (position 1-based, total, summary of the response shown) for the Response pane title.
    pub fn response_history_position(&self) -> Option<(usize, usize, &ResponseMeta)> {
        let list = self.response_history.get(&self.current.id)?;
        let i = self.response_view.unwrap_or(list.len().checked_sub(1)?);
        Some((i + 1, list.len(), list.get(i)?))
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

    /// Requests worth keeping across a restart: unsaved edits or undo history.
    /// Responses aren't included; they're reloaded from responses.jsonl.
    fn session_snapshot(&self) -> StoredSession {
        let mut drafts = HashMap::new();
        let mut keep = |id: Uuid, item: &RequestItem, history: StoredBuffers| {
            if self.is_dirty(id) || !history.is_empty() {
                drafts.insert(
                    id,
                    StoredDraft {
                        item: item.clone(),
                        response: None,
                        last_error: None,
                        history,
                    },
                );
            }
        };
        let current_loaded = self.source.is_some();
        for (id, d) in &self.drafts {
            if current_loaded && *id == self.current.id {
                continue;
            }
            keep(*id, &d.item, d.buffers.to_stored());
        }
        if current_loaded {
            let mut item = self.current.clone();
            item.url = self.url_state.lines.to_string();
            item.body = self.body_state.lines.to_string();
            item.pre_request_script = self.pre_script_state.lines.to_string();
            item.post_response_script = self.post_script_state.lines.to_string();
            item.sync_path_params();
            let history = StoredBuffers::capture([
                &self.url_state,
                &self.body_state,
                &self.pre_script_state,
                &self.post_script_state,
            ]);
            keep(self.current.id, &item, history);
        }
        StoredSession { drafts }
    }

    /// Write drafts.json, unless its content is unchanged since the last write.
    /// Write workspace.json with the next autosave (about a second after the user
    /// stops, at most ~5 s) or on quit. For bookkeeping that changes on every request
    /// switch (recent requests, open tabs, tab order), so switching tabs doesn't
    /// rewrite a multi-MB file each time.
    pub fn persist_later(&mut self) {
        self.workspace_dirty = true;
        self.mark_session_changed();
    }

    /// The autosave (debounced ticks, and on quit): pending workspace changes, then
    /// drafts.json if its content changed.
    pub fn save_session(&mut self) {
        use std::hash::{Hash, Hasher};
        self.session_changes = None;
        if self.workspace_dirty {
            self.persist();
        }
        let json = match serde_json::to_string(&self.session_snapshot()) {
            Ok(json) => json,
            Err(e) => {
                crate::logger::log("drafts serialize failed", &format!("{e:#}"));
                return;
            }
        };
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        json.hash(&mut hasher);
        let hash = hasher.finish();
        if self.last_session_hash == Some(hash) {
            return;
        }
        match crate::storage::save_session_json(&json) {
            Ok(()) => self.last_session_hash = Some(hash),
            Err(e) => {
                let detail = format!("{e:#}");
                crate::logger::log("drafts save failed", &detail);
                self.status_message = format!("Failed to save drafts: {detail}");
            }
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
                RequestDraft {
                    item: d.item,
                    response: d.response,
                    last_error: d.last_error,
                    buffers,
                },
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

/// Delete the temp file of a binary body that is no longer referenced.
fn discard_body_file(resp: Option<&ResponseData>) {
    if let Some(path) = resp.and_then(|r| r.body_file.as_ref()) {
        let _ = std::fs::remove_file(path);
    }
}

/// Largest char boundary at or below `max` (str::floor_char_boundary is unstable).
fn floor_char_boundary(s: &str, max: usize) -> usize {
    let mut cut = max.min(s.len());
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    cut
}

fn looks_like_json(body: &str) -> bool {
    matches!(body.trim_start().chars().next(), Some('{' | '['))
}

/// Split lines longer than `max` chars into `max`-char pieces. Returns the text and
/// whether anything was split.
fn split_long_lines(text: String, max: usize) -> (String, bool) {
    if !text.lines().any(|l| l.len() > max) {
        return (text, false); // byte length is an upper bound on chars: fast path
    }
    let mut out = String::with_capacity(text.len() + text.len() / max + 1);
    let mut split = false;
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        for (j, c) in line.chars().enumerate() {
            if j > 0 && j % max == 0 {
                out.push('\n');
                split = true;
            }
            out.push(c);
        }
    }
    (out, split)
}

/// "1.0 MB", "512 KB", "12 B".
pub fn human_size(bytes: usize) -> String {
    match bytes {
        b if b >= 1024 * 1024 => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
        b if b >= 1024 => format!("{} KB", b / 1024),
        b => format!("{b} B"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Collection;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
        let state = &mut app.body_state;
        app.editor_events
            .on_key_event(KeyEvent::new(code, mods), state);
    }

    use crate::test_support::isolated_storage;

    fn two_request_workspace() -> (Workspace, Uuid, Uuid) {
        let mut ws = Workspace::default();
        let mut c = Collection::new("c");
        let mut a = RequestItem::new("a");
        a.body = "x".into();
        c.requests.push(a);
        c.requests.push(RequestItem::new("b"));
        let ids = (c.requests[0].id, c.requests[1].id);
        ws.collections.push(c);
        (ws, ids.0, ids.1)
    }

    fn resp(status: u16) -> ResponseData {
        ResponseData {
            status,
            headers: vec![],
            body: format!("body {status}"),
            duration_ms: 5,
            ..Default::default()
        }
    }

    #[test]
    fn response_history_follows_the_sent_request_and_steps_back() {
        let _storage = isolated_storage();
        let (ws, a_id, b_id) = two_request_workspace();
        let mut app = App::new(ws);
        app.load_request(0, 0);
        let sent_a = app.current.clone();

        app.deliver_response(&sent_a, Some(resp(200)), None);
        app.deliver_response(&sent_a, Some(resp(201)), None);
        assert_eq!(app.response.as_ref().unwrap().status, 201);

        // `[` goes back to the first response, `]` returns to the latest.
        app.step_response_history(true);
        assert_eq!(app.response_history_position().unwrap().0, 1);
        assert!(app.response_state.lines.to_string().contains("body 200"));
        app.step_response_history(false);
        assert_eq!(app.response_view, None);
        assert!(app.response_state.lines.to_string().contains("body 201"));

        // A response arriving after switching tabs lands on the request that was sent.
        app.load_request(0, 1);
        app.deliver_response(&sent_a, Some(resp(500)), None);
        assert!(app.response.is_none(), "b's pane is untouched");
        assert_eq!(app.drafts[&a_id].response.as_ref().unwrap().status, 500);
        assert!(!app.response_history.contains_key(&b_id));

        // Reload from disk with the per-request cap applied.
        for _ in 0..RESPONSES_PER_REQUEST {
            app.deliver_response(&sent_a, Some(resp(204)), None);
        }
        let mut app2 = App::new(app.workspace.clone());
        app2.load_response_history(crate::storage::read_response_lines());
        assert_eq!(app2.response_history[&a_id].len(), RESPONSES_PER_REQUEST);
        assert!(
            crate::storage::read_response_lines().len() == RESPONSES_PER_REQUEST,
            "file compacted"
        );
    }

    #[test]
    fn manual_post_script_runs_against_the_past_response_on_screen() {
        let _storage = isolated_storage();
        let (mut ws, _, _) = two_request_workspace();
        let env = Environment::new("e");
        ws.active_environment = Some(env.id);
        ws.environments.push(env);
        let mut app = App::new(ws);
        app.load_request(0, 0);
        app.post_script_state = EditorState::new(Lines::from(
            "pm.environment:set('seen', tostring(pm.response.code))",
        ));
        let sent = app.current.clone();
        app.deliver_response(&sent, Some(resp(200)), None);
        app.deliver_response(&sent, Some(resp(201)), None);

        app.step_response_history(true); // now showing the 200
        app.run_post_script_on_shown_response();
        let seen = |app: &App| {
            app.active_environment().and_then(|e| {
                e.variables
                    .iter()
                    .find(|(k, _)| k == "seen")
                    .map(|(_, v)| v.clone())
            })
        };
        assert_eq!(seen(&app).as_deref(), Some("200"), "{}", app.status_message);
        assert_eq!(
            app.response.as_ref().unwrap().status,
            201,
            "live response untouched"
        );

        app.step_response_history(false); // back to latest
        app.run_post_script_on_shown_response();
        assert_eq!(seen(&app).as_deref(), Some("201"));
    }

    #[test]
    fn quick_slots_assign_open_and_survive_restart() {
        let _storage = isolated_storage();
        let (ws, a_id, b_id) = two_request_workspace();
        let mut app = App::new(ws);

        app.assign_slot(a_id, 1);
        app.assign_slot(b_id, 3);
        assert_eq!((app.slot_of(a_id), app.slot_of(b_id)), (Some(1), Some(3)));

        // A request holds one slot: moving `a` to 2 frees 1.
        app.assign_slot(a_id, 2);
        assert_eq!(app.slot_of(a_id), Some(2));
        assert!(!app.workspace.quick_slots.contains_key(&1));
        // A slot holds one request: taking 3 for `a` removes `b`.
        app.assign_slot(a_id, 3);
        assert_eq!((app.slot_of(a_id), app.slot_of(b_id)), (Some(3), None));
        assert!(
            app.status_message.contains("was 'b'"),
            "{}",
            app.status_message
        );

        app.open_slot(3);
        assert_eq!(app.current.id, a_id);
        app.open_slot(7);
        assert!(app.status_message.contains("empty"));

        // Saved with the workspace.
        let reloaded = crate::storage::load().unwrap();
        assert_eq!(reloaded.quick_slots.get(&3), Some(&a_id));

        app.clear_slot_of(a_id);
        assert_eq!(app.slot_of(a_id), None);

        // Slots of deleted requests are dropped.
        app.assign_slot(b_id, 5);
        app.workspace.collections[0]
            .requests
            .retain(|r| r.id != b_id);
        app.prune_tabs();
        assert!(app.workspace.quick_slots.is_empty());
    }

    #[test]
    fn sending_state_belongs_to_the_request_that_was_sent() {
        let _storage = isolated_storage();
        let (ws, _, _) = two_request_workspace();
        let mut app = App::new(ws);
        app.load_request(0, 0);
        let sent_a = app.current.clone();
        app.begin_send(sent_a.id);
        assert!(app.is_sending());

        app.load_request(0, 1);
        assert!(
            !app.is_sending(),
            "b shows no 'sending...' while a is in flight"
        );

        app.load_request(0, 0);
        assert!(app.is_sending(), "back on a, it's still sending");

        // Sent twice: still sending until both responses are in.
        app.begin_send(sent_a.id);
        app.deliver_response(&sent_a, Some(resp(200)), None);
        assert!(app.is_sending());
        app.deliver_response(&sent_a, Some(resp(201)), None);
        assert!(!app.is_sending());
        assert!(app.in_flight.is_empty());
    }

    #[test]
    fn past_responses_are_read_from_disk_and_survive_a_restart() {
        let _storage = isolated_storage();
        let (ws, a_id, _) = two_request_workspace();
        let mut app = App::new(ws.clone());
        app.load_request(0, 0);
        let sent = app.current.clone();
        app.deliver_response(&sent, Some(resp(200)), None);
        app.deliver_response(&sent, None, Some("boom".into()));
        app.deliver_response(&sent, Some(resp(201)), None);

        // Only summaries are in memory.
        let metas = &app.response_history[&a_id];
        assert_eq!(
            metas.iter().map(|m| m.status).collect::<Vec<_>>(),
            vec![Some(200), None, Some(201)]
        );

        // Stepping back reads the body from disk, including a failure.
        app.step_response_history(true);
        assert!(app.viewed_response.as_ref().unwrap().error.as_deref() == Some("boom"));
        app.step_response_history(true);
        assert!(app.response_state.lines.to_string().contains("body 200"));

        // drafts.json no longer carries responses...
        app.save_session();
        let drafts =
            std::fs::read_to_string(dirs::data_dir().unwrap().join("postui").join("drafts.json"))
                .unwrap_or_default();
        assert!(
            !drafts.contains("body 20"),
            "responses stay out of drafts.json"
        );

        // ...yet after a restart the request reopens with its latest response.
        let mut app2 = App::new(ws);
        app2.load_response_history(crate::storage::read_response_lines());
        app2.load_request(0, 0);
        assert_eq!(app2.response.as_ref().map(|r| r.status), Some(201));
        assert_eq!(app2.response_history[&a_id].len(), 3);
    }

    #[test]
    fn loading_a_request_moves_its_draft_out_instead_of_copying() {
        let _storage = isolated_storage();
        let (ws, a_id, _) = two_request_workspace();
        let mut app = App::new(ws);
        app.load_request(0, 0);
        app.load_request(0, 1); // a is stashed as a draft
        assert!(app.drafts.contains_key(&a_id));
        app.load_request(0, 0);
        assert!(
            !app.drafts.contains_key(&a_id),
            "the loaded request isn't also a draft"
        );
    }

    #[test]
    fn switching_requests_defers_the_workspace_write_to_the_autosave() {
        let _storage = isolated_storage();
        let (ws, a_id, b_id) = two_request_workspace();
        let path = dirs::data_dir()
            .unwrap()
            .join("postui")
            .join("workspace.json");
        let mut app = App::new(ws);
        app.load_request(0, 0);
        app.load_request(0, 1);
        assert!(
            !path.exists(),
            "opening requests doesn't write workspace.json right away"
        );
        assert!(
            app.session_save_due() || app.session_changes.is_some(),
            "an autosave is scheduled"
        );

        app.save_session(); // what the debounced tick (or quit) does
        let saved = crate::storage::load().unwrap();
        assert_eq!(saved.open_tabs, vec![a_id, b_id]);
        assert_eq!(saved.recent_requests.first(), Some(&b_id));

        // Important changes still go to disk immediately.
        app.assign_slot(a_id, 4);
        assert_eq!(
            crate::storage::load().unwrap().quick_slots.get(&4),
            Some(&a_id)
        );
    }

    #[test]
    fn unchanged_session_is_not_rewritten() {
        let _storage = isolated_storage();
        let (ws, _, _) = two_request_workspace();
        let mut app = App::new(ws);
        app.load_request(0, 0);
        app.body_state = EditorState::new(Lines::from("edited"));
        app.save_session();
        let path = dirs::data_dir().unwrap().join("postui").join("drafts.json");
        let first = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        app.save_session(); // nothing changed
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), first);
    }

    fn big_json(bytes: usize) -> ResponseData {
        let item = "{\"k\": \"0123456789abcdef\"},";
        let body = format!("[{}{{}}]", item.repeat(bytes / item.len()));
        ResponseData {
            status: 200,
            headers: vec![],
            body,
            duration_ms: 1,
            ..Default::default()
        }
    }

    #[test]
    fn large_responses_use_light_mode_and_small_ones_are_pretty() {
        let _storage = isolated_storage();
        let (ws, _, _) = two_request_workspace();
        let mut app = App::new(ws);
        app.load_request(0, 0);
        let sent = app.current.clone();

        app.deliver_response(&sent, Some(big_json(3 * LIGHT_MODE_BYTES)), None);
        let total = app.response.as_ref().unwrap().body.len();
        assert_eq!(app.response_light, Some(total));
        assert!(!app.response_is_json, "no highlighting in light mode");
        let shown = app.response_state.lines.to_string();
        assert!(
            shown.len() < LIGHT_MODE_BYTES + 1024,
            "only the start is loaded"
        );
        assert!(shown.contains("light mode: showing the start of"));
        assert!(
            shown.lines().count() > 1000,
            "formatted into many lines, not one huge line"
        );

        app.deliver_response(&sent, Some(resp(200)), None);
        assert_eq!(app.response_light, None);
    }

    /// edtui renders very long lines in superlinear time (a 1M-char line hung the UI),
    /// so the Response pane must never get one, whatever the body looks like.
    #[test]
    fn response_pane_never_gets_very_long_lines() {
        let _storage = isolated_storage();
        let (ws, _, _) = two_request_workspace();
        let mut app = App::new(ws);
        app.load_request(0, 0);
        let sent = app.current.clone();
        let bodies = [
            "a".repeat(900_000),              // minified, not JSON, < 1 MB
            "b".repeat(3 * LIGHT_MODE_BYTES), // same, light mode
            format!("{{\"file\": \"{}\"}}", "c".repeat(600_000)), // JSON with a huge string
            big_json(3 * LIGHT_MODE_BYTES).body, // big compact JSON
        ];
        for body in bodies {
            let r = ResponseData {
                status: 200,
                headers: vec![],
                body,
                duration_ms: 1,
                ..Default::default()
            };
            app.deliver_response(&sent, Some(r), None);
            let longest = app
                .response_state
                .lines
                .to_string()
                .lines()
                .map(|l| l.chars().count())
                .max();
            assert!(
                longest.unwrap_or(0) <= VIEW_MAX_LINE_CHARS,
                "line of {longest:?} chars"
            );
        }
    }

    #[test]
    fn save_and_pager_get_the_whole_body() {
        let _storage = isolated_storage();
        let (ws, _, _) = two_request_workspace();
        let mut app = App::new(ws);
        app.load_request(0, 0);
        let sent = app.current.clone();
        let big = big_json(2 * LIGHT_MODE_BYTES);
        app.deliver_response(&sent, Some(big.clone()), None);

        let dir = std::env::temp_dir().join(format!("postui-save-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("body.json");
        assert_eq!(
            app.save_response_to(target.to_str().unwrap(), false),
            SaveResult::Saved
        );
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            big.body,
            "saved as received"
        );

        app.open_shown_response_in_pager();
        let pager_file = app.pending_pager.take().expect("pager requested");
        let text = std::fs::read_to_string(&pager_file).unwrap();
        assert!(text.len() > big.body.len(), "pretty-printed, not truncated");
        let _ = std::fs::remove_file(pager_file);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A binary response streamed to a temp file: saving copies its exact bytes,
    /// names it from Content-Disposition, remembers the folder per request, and asks
    /// before overwriting.
    #[test]
    fn binary_bodies_save_byte_for_byte_and_remember_the_folder() {
        let _storage = isolated_storage();
        let (ws, a_id, _) = two_request_workspace();
        let mut app = App::new(ws);
        app.load_request(0, 0);
        let sent = app.current.clone();

        let bytes: Vec<u8> = (0..=255u8).cycle().take(300_000).collect(); // not UTF-8
        let tmp = crate::storage::bodies_dir().unwrap().join("test.bin");
        std::fs::write(&tmp, &bytes).unwrap();
        let zip = ResponseData {
            status: 200,
            headers: vec![
                ("content-type".into(), "application/zip".into()),
                (
                    "content-disposition".into(),
                    r#"attachment; filename="documents.zip""#.into(),
                ),
            ],
            binary: true,
            size: bytes.len() as u64,
            body_file: Some(tmp.clone()),
            ..Default::default()
        };
        app.deliver_response(&sent, Some(zip), None);
        assert!(app.response_state.lines.to_string().contains("Binary body"));

        let dir = std::env::temp_dir().join(format!("postui-save-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let folder = format!("{}/", dir.display());
        assert_eq!(app.save_response_to(&folder, false), SaveResult::Saved);
        let saved = dir.join("documents.zip");
        assert_eq!(std::fs::read(&saved).unwrap(), bytes, "exact bytes");

        // Remembered per request, and saved immediately with the workspace.
        assert_eq!(
            crate::storage::load()
                .unwrap()
                .response_save_paths
                .get(&a_id)
                .map(String::as_str),
            Some(saved.display().to_string().as_str())
        );
        let suggested = crate::save::expand_tilde(&app.suggested_save_path());
        assert_eq!(suggested, saved, "same folder, server's file name");

        // Same name again: not overwritten without confirmation.
        assert_eq!(
            app.save_response_to(saved.to_str().unwrap(), false),
            SaveResult::Exists(saved.clone())
        );
        assert_eq!(
            crate::save::next_free_path(&saved),
            dir.join("documents (1).zip")
        );
        assert_eq!(
            app.save_response_to(saved.to_str().unwrap(), true),
            SaveResult::Saved
        );

        // Replacing the response frees the old temp file.
        app.deliver_response(&sent, Some(resp(200)), None);
        assert!(!tmp.exists(), "temp body removed");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unsaved_edits_and_undo_survive_tab_switch_and_restart() {
        let _storage = isolated_storage();

        let (ws, a_id, b_id) = two_request_workspace();
        let mut app = App::new(ws);
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
        assert_eq!(
            app2.body_state.lines.to_string(),
            "x",
            "undo history restored"
        );
        press(&mut app2, KeyCode::Char('r'), KeyModifiers::CONTROL);
        assert_eq!(app2.body_state.lines.to_string(), "xyz", "redo works too");

        // space W writes every dirty request back.
        app2.save_all();
        assert!(!app2.is_dirty(a_id));
        assert_eq!(app2.workspace.collections[0].requests[0].body, "xyz");
    }
}
