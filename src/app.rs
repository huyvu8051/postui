use std::collections::{HashMap, HashSet};

use edtui::{EditorEventHandler, EditorMode, EditorState, Lines};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use uuid::Uuid;

use crate::model::{Environment, FormField, Header, HistoryEntry, RequestItem, ResponseData, Workspace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Method,
    Url,
    PathParams,
    Headers,
    BodyMode,
    Body,
    ScriptTab,
    ScriptEditor,
    Response,
}

impl Focus {
    pub fn neighbor(self, dir: Direction) -> Option<Focus> {
        use Direction::{Down, Left, Right, Up};
        match (self, dir) {
            (Focus::Method, Right) => Some(Focus::Url),
            (Focus::Method, Down) => Some(Focus::PathParams),
            (Focus::Url, Left) => Some(Focus::Method),
            (Focus::Url, Down) => Some(Focus::PathParams),
            (Focus::PathParams, Up) => Some(Focus::Method),
            (Focus::PathParams, Down) => Some(Focus::Headers),
            (Focus::Headers, Up) => Some(Focus::PathParams),
            (Focus::Headers, Down) => Some(Focus::BodyMode),
            (Focus::BodyMode, Up) => Some(Focus::Headers),
            (Focus::BodyMode, Right) => Some(Focus::ScriptTab),
            (Focus::BodyMode, Down) => Some(Focus::Body),
            (Focus::Body, Up) => Some(Focus::BodyMode),
            (Focus::Body, Right) => Some(Focus::ScriptEditor),
            (Focus::ScriptTab, Left) => Some(Focus::BodyMode),
            (Focus::ScriptTab, Right) => Some(Focus::Response),
            (Focus::ScriptTab, Up) => Some(Focus::Headers),
            (Focus::ScriptTab, Down) => Some(Focus::ScriptEditor),
            (Focus::ScriptEditor, Left) => Some(Focus::Body),
            (Focus::ScriptEditor, Right) => Some(Focus::Response),
            (Focus::ScriptEditor, Up) => Some(Focus::ScriptTab),
            (Focus::Response, Left) => Some(Focus::ScriptEditor),
            (Focus::Response, Up) => Some(Focus::Headers),
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
    AwaitingS,
    AwaitingF,
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
    CollectionFinder {
        query: EditorState,
        selected: usize,
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
}

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
    pub panel_mode: PanelMode,
    pub maximized: bool,
    pub editor_events: EditorEventHandler,

    pub url_state: EditorState,
    pub body_state: EditorState,

    pub script_tab: ScriptTab,
    pub pre_script_state: EditorState,
    pub post_script_state: EditorState,
    pub script_console: Vec<String>,

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
            panel_mode: PanelMode::default(),
            maximized: false,
            editor_events: EditorEventHandler::default(),
            url_state,
            body_state,
            script_tab: ScriptTab::default(),
            pre_script_state: EditorState::default(),
            post_script_state: EditorState::default(),
            script_console: Vec::new(),
            header_index: 0,
            form_field_index: 0,
            path_param_index: 0,
            response: None,
            last_error: None,
            response_state: EditorState::default(),
            response_is_json: false,
            loading: false,
            status_message: String::from(
                "hjkl: select pane | Enter: focus, Esc: unfocus | space s f: find/open request | Ctrl+Enter: send | e: environments | q: quit",
            ),
            history: Vec::new(),
            env_index: 0,
            env_var_index: 0,
            popup: Popup::None,
            should_quit: false,
        }
    }

    pub fn move_focus(&mut self, dir: Direction) {
        if let Some(next) = self.focus.neighbor(dir) {
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

    pub fn collection_finder_matches(&self, query: &str) -> Vec<SidebarEntry> {
        if query.trim().is_empty() {
            return self.flatten_sidebar();
        }
        self.fuzzy_ranked_requests(query)
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
    }

    fn drop_oldest_script_console_lines_past_limit(&mut self) {
        let excess = self.script_console.len().saturating_sub(SCRIPT_CONSOLE_LIMIT);
        self.script_console.drain(0..excess);
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
            None => self.restore_saved_item(item),
        }
        self.current.sync_path_params();
        self.source = Some(RequestSource { collection_index: coll_idx, request_index: req_idx });
        self.sync_editors_from_current();
        self.header_index = 0;
        self.form_field_index = 0;
        self.path_param_index = 0;
        self.refresh_response_view();
        self.status_message = format!("Loaded '{}'", self.current.name);
    }

    fn restore_draft(&mut self, draft: RequestDraft) {
        self.current = draft.item;
        self.response = draft.response;
        self.last_error = draft.last_error;
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
            },
        );
    }

    pub fn save_current(&mut self) {
        self.sync_current_from_editors();
        let name = match self.source.and_then(|source| {
            self.workspace
                .collections
                .get_mut(source.collection_index)
                .and_then(|c| c.requests.get_mut(source.request_index))
        }) {
            Some(r) => {
                let id = r.id;
                *r = self.current.clone();
                r.id = id;
                r.name.clone()
            }
            None => {
                self.status_message =
                    "No collection request loaded — open <space> s f and Ctrl+N to create one first"
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

    pub fn push_history(&mut self, status: Option<u16>) {
        self.history.push(HistoryEntry {
            timestamp: chrono::Local::now(),
            method: self.current.method,
            url: self.current.url.clone(),
            status,
        });
    }
}
