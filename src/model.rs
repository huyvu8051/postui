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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestItem {
    pub id: Uuid,
    pub name: String,
    pub method: Method,
    pub url: String,
    /// Path variables, e.g. `:tenantId` in `{{baseUrl}}/:tenantId/baskets`.
    /// Auto-derived from `url` (see `sync_path_params`) — key order and
    /// membership follow the URL, only the values are hand-edited.
    #[serde(default)]
    pub path_params: Vec<(String, String)>,
    pub headers: Vec<Header>,
    pub body: String,
}

/// Names of `:paramName` path segments in `url`, in order of first
/// appearance, deduplicated. Only a `:` that starts a whole `/`-delimited
/// segment counts, so `https://host:8080/path` yields no params.
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
        }
    }

    /// Reconcile `path_params` with whatever `:name` segments are currently
    /// in `url`: keep values for params still referenced, drop params no
    /// longer referenced, append newly-referenced ones with an empty value —
    /// in URL order. Call after any edit to `url`.
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

    /// Substitute `:name` path segments in `url` using this request's
    /// `path_params` values.
    pub fn resolve_url(&self, url: &str) -> String {
        url.split('/')
            .map(|segment| {
                let Some(name) = segment.strip_prefix(':') else {
                    return segment.to_string();
                };
                match self.path_params.iter().find(|(k, _)| k == name) {
                    Some((_, v)) => v.clone(),
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

    /// Substitute {{var}} occurrences using this environment's variables.
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
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub timestamp: chrono::DateTime<chrono::Local>,
    pub method: Method,
    pub url: String,
    pub status: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct ResponseData {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub duration_ms: u128,
}
