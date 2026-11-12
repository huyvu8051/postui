use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl Method {
    pub const ALL: [Method; 7] = [
        Method::Get,
        Method::Post,
        Method::Put,
        Method::Patch,
        Method::Delete,
        Method::Head,
        Method::Options,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Head => "HEAD",
            Method::Options => "OPTIONS",
        }
    }

    pub fn next(&self) -> Method {
        let idx = Method::ALL.iter().position(|m| m == self).unwrap();
        Method::ALL[(idx + 1) % Method::ALL.len()]
    }

    pub fn prev(&self) -> Method {
        let idx = Method::ALL.iter().position(|m| m == self).unwrap();
        Method::ALL[(idx + Method::ALL.len() - 1) % Method::ALL.len()]
    }

    pub fn to_reqwest(&self) -> reqwest::Method {
        match self {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Patch => reqwest::Method::PATCH,
            Method::Delete => reqwest::Method::DELETE,
            Method::Head => reqwest::Method::HEAD,
            Method::Options => reqwest::Method::OPTIONS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

impl Header {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BodyMode {
    #[default]
    Raw,
    FormData,
    Binary,
}

impl BodyMode {
    pub const ALL: [BodyMode; 3] = [BodyMode::Raw, BodyMode::FormData, BodyMode::Binary];

    pub fn as_str(&self) -> &'static str {
        match self {
            BodyMode::Raw => "Raw",
            BodyMode::FormData => "Form Data",
            BodyMode::Binary => "Binary",
        }
    }

    pub fn next(&self) -> BodyMode {
        let idx = BodyMode::ALL.iter().position(|m| m == self).unwrap();
        BodyMode::ALL[(idx + 1) % BodyMode::ALL.len()]
    }

    pub fn prev(&self) -> BodyMode {
        let idx = BodyMode::ALL.iter().position(|m| m == self).unwrap();
        BodyMode::ALL[(idx + BodyMode::ALL.len() - 1) % BodyMode::ALL.len()]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FormValue {
    Text(String),
    File(String),
}

impl FormValue {
    pub fn as_str(&self) -> &str {
        match self {
            FormValue::Text(s) | FormValue::File(s) => s,
        }
    }

    pub fn is_file(&self) -> bool {
        matches!(self, FormValue::File(_))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub key: String,
    pub value: FormValue,
    pub enabled: bool,
}

impl FormField {
    pub fn new_text(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: FormValue::Text(value.into()),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestItem {
    pub id: Uuid,
    pub name: String,
    pub method: Method,
    pub url: String,
    #[serde(default)]
    pub path_params: Vec<(String, String)>,
    pub headers: Vec<Header>,
    pub body: String,
    #[serde(default)]
    pub body_mode: BodyMode,
    #[serde(default)]
    pub form_fields: Vec<FormField>,
    #[serde(default)]
    pub binary_file: String,
    #[serde(default)]
    pub pre_request_script: String,
    #[serde(default)]
    pub post_response_script: String,
}

pub fn extract_path_param_names(url: &str) -> Vec<String> {
    let mut names = Vec::new();
    for segment in url.split('/') {
        let Some(name) = segment.strip_prefix(':') else {
            continue;
        };
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        if !names.iter().any(|n: &String| n == name) {
            names.push(name.to_string());
        }
    }
    names
}

impl RequestItem {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            method: Method::Get,
            url: String::new(),
            path_params: Vec::new(),
            headers: Vec::new(),
            body: String::new(),
            body_mode: BodyMode::default(),
            form_fields: Vec::new(),
            binary_file: String::new(),
            pre_request_script: String::new(),
            post_response_script: String::new(),
        }
    }

    pub fn sync_path_params(&mut self) {
        let names = extract_path_param_names(&self.url);
        let mut synced = Vec::with_capacity(names.len());
        for name in names {
            let value = self
                .path_params
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            synced.push((name, value));
        }
        self.path_params = synced;
    }

    pub fn resolve_url(&self, url: &str, resolve: &impl Fn(&str) -> String) -> String {
        url.split('/')
            .map(|segment| {
                let Some(name) = segment.strip_prefix(':') else {
                    return segment.to_string();
                };
                match self.path_params.iter().find(|(k, _)| k == name) {
                    Some((_, v)) => resolve(v),
                    None => segment.to_string(),
                }
            })
            .collect::<Vec<_>>()
            .join("/")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: Uuid,
    pub name: String,
    pub requests: Vec<RequestItem>,
}

impl Collection {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            requests: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub id: Uuid,
    pub name: String,
    pub variables: Vec<(String, String)>,
}

impl Environment {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            variables: Vec::new(),
        }
    }

    pub fn resolve(&self, input: &str) -> String {
        let mut out = input.to_string();
        for (k, v) in &self.variables {
            let pattern = format!("{{{{{}}}}}", k);
            out = out.replace(&pattern, v);
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Workspace {
    pub collections: Vec<Collection>,
    pub environments: Vec<Environment>,
    pub active_environment: Option<Uuid>,
    /// Request ids, most recently opened first. Shown by the finder when its query is empty.
    #[serde(default)]
    pub recent_requests: Vec<Uuid>,
    /// Requests open as tabs in the sidebar, in opening order.
    #[serde(default)]
    pub open_tabs: Vec<Uuid>,
}

/// One sent request, newest last. `status` is None when the request failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: chrono::DateTime<chrono::Local>,
    #[serde(default)]
    pub request_id: Option<Uuid>,
    #[serde(default)]
    pub name: String,
    pub method: Method,
    pub url: String,
    pub status: Option<u16>,
    #[serde(default)]
    pub duration_ms: Option<u128>,
}

/// One past response (or failure) of a request, for browsing with `[` / `]` in the
/// Response pane. Stored one per line in responses.jsonl.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseRecord {
    pub request_id: Uuid,
    pub timestamp: chrono::DateTime<chrono::Local>,
    #[serde(default)]
    pub response: Option<ResponseData>,
    #[serde(default)]
    pub error: Option<String>,
    /// Body was cut to the size limit before storing.
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseData {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub duration_ms: u128,
}
