//! Saving a response body to a file: the suggested file name, Tab completion for the
//! path input, and finding a free name when the file already exists.
//!
//! Everything here is pure (no `App`), so the rules are easy to test.
use std::path::{Path, PathBuf};

/// File name from a `Content-Disposition` header, if it has one. Handles
/// `filename*=UTF-8''…` (RFC 5987, percent-encoded) and `filename="…"` / `filename=…`.
/// Only the last path component is kept, so a server can't direct the save outside
/// the folder the user picked (`../../.ssh/x` becomes `x`).
pub fn filename_from_content_disposition(header: &str) -> Option<String> {
    let mut plain = None;
    let mut extended = None;
    for part in header.split(';').map(str::trim) {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "filename*" => {
                // charset'language'percent-encoded-value
                let encoded = value.trim().splitn(3, '\'').nth(2).unwrap_or(value.trim());
                extended = Some(percent_decode(encoded.trim_matches('"')));
            }
            "filename" => plain = Some(value.trim().trim_matches('"').to_string()),
            _ => {}
        }
    }
    sanitize_file_name(&extended.or(plain)?)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Keep only a safe last path component: no separators, no `.`/`..`, no control chars.
pub fn sanitize_file_name(name: &str) -> Option<String> {
    let last = name.rsplit(['/', '\\']).next()?.trim();
    let clean: String = last.chars().filter(|c| !c.is_control()).collect();
    if clean.is_empty() || clean == "." || clean == ".." {
        return None;
    }
    Some(clean)
}

/// A file extension for a `Content-Type`, used when the server doesn't name the file.
pub fn extension_for_content_type(content_type: Option<&str>, binary: bool) -> &'static str {
    let mime = content_type
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match mime.as_str() {
        "application/zip" | "application/x-zip-compressed" => "zip",
        "application/pdf" => "pdf",
        "application/gzip" | "application/x-gzip" => "gz",
        "application/x-tar" => "tar",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        "image/webp" => "webp",
        "text/csv" => "csv",
        "text/html" => "html",
        "text/plain" => "txt",
        "application/xml" | "text/xml" => "xml",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        m if m.ends_with("json") => "json",
        _ if binary => "bin",
        _ => "txt",
    }
}

/// `report.zip` → `report (1).zip`, `report (2).zip`, … — the first name in `dir`
/// that doesn't exist yet.
pub fn next_free_path(path: &Path) -> PathBuf {
    let dir = path.parent().unwrap_or(Path::new("."));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("some numbered name is free")
}

/// `~/x` → `$HOME/x`; other paths unchanged.
pub fn expand_tilde(input: &str) -> PathBuf {
    match input.strip_prefix("~/") {
        Some(rest) => dirs::home_dir().map_or_else(|| PathBuf::from(input), |h| h.join(rest)),
        None if input == "~" => dirs::home_dir().unwrap_or_else(|| PathBuf::from(input)),
        None => PathBuf::from(input),
    }
}

/// Shell-style completions for a path being typed: every entry of the directory part
/// whose name starts with the last component, as full input strings (directories end
/// with `/`, hidden entries only when the prefix starts with `.`), sorted.
pub fn complete_path(input: &str) -> Vec<String> {
    let (dir_part, prefix) = match input.rfind('/') {
        Some(i) => (&input[..=i], &input[i + 1..]),
        None => ("", input),
    };
    let dir = if dir_part.is_empty() {
        PathBuf::from(".")
    } else {
        expand_tilde(dir_part)
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with(prefix) || (name.starts_with('.') && !prefix.starts_with('.')) {
                return None;
            }
            let is_dir = e.file_type().is_ok_and(|t| t.is_dir())
                || (e.file_type().is_ok_and(|t| t.is_symlink()) && e.path().is_dir());
            Some(format!("{dir_part}{name}{}", if is_dir { "/" } else { "" }))
        })
        .collect();
    out.sort();
    out
}

/// Longest common prefix of the completions (what a first Tab fills in).
pub fn common_prefix(items: &[String]) -> String {
    let Some(first) = items.first() else {
        return String::new();
    };
    let mut end = first.len();
    for item in &items[1..] {
        end = first
            .char_indices()
            .zip(item.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map_or(0, |((i, a), _)| i + a.len_utf8())
            .min(end);
    }
    first[..end].to_string()
}

/// Tab completion state for the save popup: the first Tab completes as far as the
/// candidates agree; pressing Tab again (without typing) cycles through them.
#[derive(Debug, Clone, Default)]
pub struct PathCompletion {
    pub candidates: Vec<String>,
    /// Index of the candidate currently filled in while cycling.
    pub index: Option<usize>,
    /// What the previous Tab produced; Tab on exactly that text continues cycling.
    last: Option<String>,
}

impl PathCompletion {
    /// Returns the new input text for a Tab press on `input`.
    pub fn tab(&mut self, input: &str) -> String {
        let continuing = self.last.as_deref() == Some(input) && self.candidates.len() > 1;
        let result = if continuing {
            let next = self.index.map_or(0, |i| (i + 1) % self.candidates.len());
            self.index = Some(next);
            self.candidates[next].clone()
        } else {
            self.candidates = complete_path(input);
            self.index = None;
            match self.candidates.len() {
                0 => input.to_string(),
                1 => {
                    let only = self.candidates[0].clone();
                    if only.ends_with('/') {
                        self.candidates.clear(); // next Tab completes inside it
                    }
                    only
                }
                _ => {
                    let prefix = common_prefix(&self.candidates);
                    if prefix.len() > input.len() {
                        prefix
                    } else {
                        self.index = Some(0);
                        self.candidates[0].clone()
                    }
                }
            }
        };
        self.last = Some(result.clone());
        result
    }

    /// Any edit other than Tab starts a fresh completion.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_disposition_names() {
        let f = filename_from_content_disposition;
        assert_eq!(
            f(r#"attachment; filename="documents.zip""#).as_deref(),
            Some("documents.zip")
        );
        assert_eq!(
            f("attachment; filename=report.pdf").as_deref(),
            Some("report.pdf")
        );
        assert_eq!(
            f(r#"attachment; filename="fallback.zip"; filename*=UTF-8''h%E1%BB%93%20s%C6%A1.zip"#)
                .as_deref(),
            Some("hồ sơ.zip"),
            "filename* wins and is percent-decoded"
        );
        assert_eq!(f("inline").as_deref(), None);
    }

    #[test]
    fn server_names_cannot_escape_the_chosen_folder() {
        let f = filename_from_content_disposition;
        assert_eq!(
            f(r#"attachment; filename="../../.ssh/authorized_keys""#).as_deref(),
            Some("authorized_keys")
        );
        assert_eq!(
            f(r#"attachment; filename="C:\\Windows\\evil.dll""#).as_deref(),
            Some("evil.dll")
        );
        assert_eq!(f(r#"attachment; filename="..""#), None);
        assert_eq!(
            f("attachment; filename=\"a\u{0}b.txt\"").as_deref(),
            Some("ab.txt")
        );
    }

    #[test]
    fn extensions() {
        assert_eq!(
            extension_for_content_type(Some("application/zip"), true),
            "zip"
        );
        assert_eq!(
            extension_for_content_type(Some("application/vnd.api+json; charset=utf-8"), false),
            "json"
        );
        assert_eq!(
            extension_for_content_type(Some("application/octet-stream"), true),
            "bin"
        );
        assert_eq!(extension_for_content_type(None, false), "txt");
    }

    fn temp_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("postui-save-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn next_free_name_numbers_like_browsers() {
        let d = temp_dir();
        std::fs::write(d.join("docs.zip"), "x").unwrap();
        std::fs::write(d.join("docs (1).zip"), "x").unwrap();
        assert_eq!(next_free_path(&d.join("docs.zip")), d.join("docs (2).zip"));
        assert_eq!(next_free_path(&d.join("README")), d.join("README (1)"));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn tab_completes_then_cycles() {
        let d = temp_dir();
        std::fs::create_dir(d.join("downloads")).unwrap();
        std::fs::write(d.join("data-1.zip"), "").unwrap();
        std::fs::write(d.join("data-2.zip"), "").unwrap();
        std::fs::write(d.join(".hidden"), "").unwrap();
        let base = format!("{}/", d.display());

        let mut c = PathCompletion::default();
        assert_eq!(
            c.tab(&format!("{base}do")),
            format!("{base}downloads/"),
            "unique dir"
        );
        c.reset();
        assert_eq!(
            c.tab(&format!("{base}da")),
            format!("{base}data-"),
            "common prefix first"
        );
        assert_eq!(
            c.tab(&format!("{base}data-")),
            format!("{base}data-1.zip"),
            "then cycle"
        );
        assert_eq!(
            c.tab(&format!("{base}data-1.zip")),
            format!("{base}data-2.zip")
        );
        assert_eq!(
            c.tab(&format!("{base}data-2.zip")),
            format!("{base}data-1.zip")
        );
        c.reset();
        assert!(
            !complete_path(&base).iter().any(|p| p.ends_with(".hidden")),
            "hidden only on '.'"
        );
        std::fs::remove_dir_all(d).unwrap();
    }
}
