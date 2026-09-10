use std::collections::{HashMap, HashSet};

use edtui::{EditorEventHandler, EditorMode, EditorState, Lines};
use ratatui::widgets::ListState;
use uuid::Uuid;

use crate::model::{Environment, FormField, Header, HistoryEntry, RequestItem, ResponseData, Workspace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Method,
    Url,
    PathParams,
    Headers,
    /// The Raw/Form Data/Binary selector above the Body pane.
    BodyMode,
    /// What this pane shows/does depends on `RequestItem::body_mode` — see
    /// `event::is_vim_editor_focus` and `ui::draw_body_content`.
    Body,
    Response,
}

impl Focus {
    pub fn next(self) -> Focus {
        match self {
            Focus::Sidebar => Focus::Method,
            Focus::Method => Focus::Url,
            Focus::Url => Focus::PathParams,
            Focus::PathParams => Focus::Headers,
            Focus::Headers => Focus::BodyMode,
            Focus::BodyMode => Focus::Body,
            Focus::Body => Focus::Response,
            Focus::Response => Focus::Sidebar,
        }
    }

    pub fn prev(self) -> Focus {
        match self {
            Focus::Sidebar => Focus::Response,
            Focus::Method => Focus::Sidebar,
            Focus::Url => Focus::Method,
            Focus::PathParams => Focus::Url,
            Focus::Headers => Focus::PathParams,
            Focus::BodyMode => Focus::Headers,
            Focus::Body => Focus::BodyMode,
            Focus::Response => Focus::Body,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarEntry {
    Collection(usize),
    Request(usize, usize),
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
    },
    EnvManager,
    EnvVarEditor {
        env_index: usize,
    },
    /// Edit the value of an auto-detected `:name` path param. The key is
    /// derived from the URL, so only the value is editable here.
    PathParamForm {
        name: String,
        value: EditorState,
        index: usize,
    },
}

/// A fresh single-line `EditorState` seeded with `text`, ready to type into
/// immediately (mode forced to `Insert` — see the "Popup fields" note in
/// `event.rs` for why we don't let these run edtui's full Normal-mode vim
/// keymap).
pub fn single_line_state(text: &str) -> EditorState {
    let mut state = EditorState::new(Lines::from(text));
    state.set_single_line(true);
    state.mode = EditorMode::Insert;
    state
}

/// A request's unsaved, in-memory state: the edited fields plus its last
/// response/error, keyed by `RequestItem::id` (stable across edits — only
/// `Ctrl+S` writes it back to the collection on disk). Letting you switch
/// away to another request and back without losing either.
pub struct RequestDraft {
    pub item: RequestItem,
    pub response: Option<ResponseData>,
    pub last_error: Option<String>,
}

pub struct App {
    pub workspace: Workspace,
    pub expanded: HashSet<Uuid>,
    pub drafts: HashMap<Uuid, RequestDraft>,

    pub sidebar_index: usize,
    pub sidebar_state: ListState,

    pub current: RequestItem,
    pub source: Option<(usize, usize)>, // (collection_idx, request_idx) that `current` was loaded from

    pub focus: Focus,
    /// When true, the focused pane fills the whole main area (no sidebar,
    /// no other panes) — toggled with `F3`.
    pub maximized: bool,
    /// Shared vim/emacs keymap for every edtui-backed field. Stateless
    /// besides the keymap itself — the actual editing state lives in each
    /// field's own `EditorState`.
    pub editor_events: EditorEventHandler,

    pub url_state: EditorState,
    pub body_state: EditorState,

    pub header_index: usize,
    pub form_field_index: usize,
    pub path_param_index: usize,
    pub response: Option<ResponseData>,
    pub last_error: Option<String>,

    /// The Response pane's buffer — always populated (rebuilt whenever a
    /// new response or error arrives), always a real edtui vim buffer:
    /// `hjkl`, `gg`/`G`, `/` search + `n`/`N`, `v` visual + `y` yank (system
    /// clipboard), all native to edtui. Syntax-highlighted at render time
    /// when the body is JSON.
    pub response_state: EditorState,
    pub response_is_json: bool,

    pub loading: bool,
    pub status_message: String,
    pub history: Vec<HistoryEntry>,

    pub env_index: usize,
    pub env_var_index: usize,

    pub popup: Popup,

    pub should_quit: bool,
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
            sidebar_index: 0,
            sidebar_state: ListState::default(),
            current: RequestItem::new("untitled"),
            source: None,
            focus: Focus::Sidebar,
            maximized: false,
            editor_events: EditorEventHandler::default(), // vim mode
            url_state,
            body_state,
            header_index: 0,
            form_field_index: 0,
            path_param_index: 0,
            response: None,
            last_error: None,
            response_state: EditorState::default(),
            response_is_json: false,
            loading: false,
            status_message: String::from(
                "Tab: switch focus | a: new request | A: new collection | Ctrl+Enter: send | e: environments | q: quit",
            ),
            history: Vec::new(),
            env_index: 0,
            env_var_index: 0,
            popup: Popup::None,
            should_quit: false,
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

    pub fn active_environment(&self) -> Option<&Environment> {
        self.workspace
            .active_environment
            .and_then(|id| self.workspace.environments.iter().find(|e| e.id == id))
    }

    pub fn sync_current_from_editors(&mut self) {
        self.current.url = self.url_state.lines.to_string();
        self.current.body = self.body_state.lines.to_string();
        self.current.sync_path_params();
        if self.path_param_index >= self.current.path_params.len() {
            self.path_param_index = self.current.path_params.len().saturating_sub(1);
        }
    }

    pub fn sync_editors_from_current(&mut self) {
        self.url_state = EditorState::new(Lines::from(self.current.url.as_str()));
        self.url_state.set_single_line(true);
        self.body_state = EditorState::new(Lines::from(self.current.body.as_str()));
    }

    pub fn load_request(&mut self, coll_idx: usize, req_idx: usize) {
        self.stash_current_draft();

        if let Some(c) = self.workspace.collections.get(coll_idx) {
            if let Some(r) = c.requests.get(req_idx) {
                match self.drafts.get(&r.id) {
                    // Reopening a request visited earlier this session:
                    // restore whatever was there, edited or not, including
                    // its last response/error — not the on-disk copy.
                    Some(draft) => {
                        self.current = draft.item.clone();
                        self.response = draft.response.clone();
                        self.last_error = draft.last_error.clone();
                    }
                    None => {
                        self.current = r.clone();
                        self.response = None;
                        self.last_error = None;
                    }
                }
                self.current.sync_path_params();
                self.source = Some((coll_idx, req_idx));
                self.sync_editors_from_current();
                self.header_index = 0;
                self.form_field_index = 0;
                self.path_param_index = 0;
                self.refresh_response_view();
                self.status_message = format!("Loaded '{}'", self.current.name);
            }
        }
    }

    /// Snapshot the currently-open request (edited or not) plus its last
    /// response/error into `drafts`, keyed by its stable id. Call before
    /// switching `current` to anything else.
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
            },
        );
    }

    pub fn save_current(&mut self) {
        self.sync_current_from_editors();
        let name = match self.source.and_then(|(ci, ri)| {
            self.workspace
                .collections
                .get_mut(ci)
                .and_then(|c| c.requests.get_mut(ri))
        }) {
            Some(r) => {
                let id = r.id;
                *r = self.current.clone();
                r.id = id;
                r.name.clone()
            }
            None => {
                self.status_message =
                    "No collection request loaded — use 'a' in the sidebar to create one first"
                        .into();
                return;
            }
        };
        self.persist();
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

    /// Rebuild the Response pane's buffer from the current `response`/
    /// `last_error`. Called whenever either changes. The buffer is a normal
    /// edtui vim buffer the whole time — there's no separate "enter/exit
    /// view mode" step; you just focus the pane and navigate. Search/
    /// selection state naturally survives losing and regaining focus
    /// because we only rebuild here, on a new response — not on refocus.
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

    /// Persist the workspace to disk, surfacing any failure (disk full,
    /// permissions, etc.) in the status bar and log instead of swallowing it.
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

    pub fn push_history(&mut self, status: Option<u16>) {
        self.history.push(HistoryEntry {
            timestamp: chrono::Local::now(),
            method: self.current.method,
            url: self.current.url.clone(),
            status,
        });
    }
}
