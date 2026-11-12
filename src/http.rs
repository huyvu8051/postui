use anyhow::{Context, Result};
use std::time::Instant;
use tokio::io::AsyncWriteExt;

use crate::model::{
    ActualBody, ActualRequest, BodyMode, Environment, FormValue, RequestItem, ResponseData,
};

/// The result of a send: exactly what was put on the wire (when the request got as
/// far as being built), and the response or error.
pub struct SendOutcome {
    pub actual: Option<ActualRequest>,
    pub result: Result<ResponseData>,
}

pub async fn send_request(req: &RequestItem, env: Option<&Environment>) -> SendOutcome {
    let mut actual = None;
    let result = send(req, env, &mut actual).await;
    SendOutcome { actual, result }
}

async fn send(
    req: &RequestItem,
    env: Option<&Environment>,
    actual: &mut Option<ActualRequest>,
) -> Result<ResponseData> {
    let resolve = |s: &str| -> String {
        match env {
            Some(e) => e.resolve(s),
            None => s.to_string(),
        }
    };

    let url = req.resolve_url(&resolve(&req.url), &resolve);
    if url::Url::parse(&url).is_err() {
        anyhow::bail!(
            "'{url}' is not a valid URL (missing scheme? try https://...) \
             — check for unresolved {{{{var}}}} placeholders (see the environment manager, space e) \
             or empty :param values (see the Path Params panel)"
        );
    }

    let client = reqwest::Client::builder()
        .build()
        .context("failed to build HTTP client")?;
    let mut builder = client.request(req.method.to_reqwest(), &url);

    for h in &req.headers {
        if h.enabled && !h.key.is_empty() {
            builder = builder.header(resolve(&h.key), resolve(&h.value));
        }
    }

    let (builder, body) = apply_body(builder, req, &resolve).await?;
    let request = builder.build().context("failed to build the request")?;
    *actual = Some(describe(&request, body, env));

    let start = Instant::now();
    let resp = client
        .execute(request)
        .await
        .with_context(|| format!("{} {url} failed", req.method.as_str()))?;
    let status = resp.status().as_u16();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let mut response = ResponseData {
        status,
        headers,
        ..Default::default()
    };
    read_body(resp, &mut response)
        .await
        .context("request succeeded but reading the response body failed")?;
    response.duration_ms = start.elapsed().as_millis();
    Ok(response)
}

/// Read the body in chunks. Text stays in memory; a body that is binary by its
/// headers is streamed to a temp file as it arrives, so even a multi-GB download
/// doesn't grow memory. Text that turns out not to be UTF-8 is moved to a file too.
async fn read_body(mut resp: reqwest::Response, out: &mut ResponseData) -> Result<()> {
    let mut file: Option<(tokio::fs::File, std::path::PathBuf)> = None;
    let mut text = Vec::new();
    if looks_binary(out) {
        file = Some(new_body_file().await?);
    }
    while let Some(chunk) = resp.chunk().await? {
        out.size += chunk.len() as u64;
        match &mut file {
            Some((f, _)) => f.write_all(&chunk).await?,
            None => text.extend_from_slice(&chunk),
        }
    }
    if file.is_none() {
        match String::from_utf8(text) {
            Ok(body) => {
                out.body = body;
                return Ok(());
            }
            Err(e) => {
                let mut f = new_body_file().await?;
                f.0.write_all(e.as_bytes()).await?;
                file = Some(f);
            }
        }
    }
    if let Some((mut f, path)) = file {
        f.flush().await?;
        out.binary = true;
        out.body_file = Some(path);
    }
    Ok(())
}

/// Binary by its headers: an attachment, or a content type that isn't text.
/// No content type at all means "try text" (and fall back to binary if not UTF-8).
fn looks_binary(resp: &ResponseData) -> bool {
    if resp.header("content-disposition").is_some_and(|d| {
        d.trim_start()
            .to_ascii_lowercase()
            .starts_with("attachment")
    }) {
        return true;
    }
    let Some(ct) = resp.header("content-type") else {
        return false;
    };
    let ct = ct.to_ascii_lowercase();
    let textual = ct.starts_with("text/")
        || [
            "json",
            "xml",
            "javascript",
            "yaml",
            "csv",
            "html",
            "graphql",
            "x-www-form-urlencoded",
            "event-stream",
        ]
        .iter()
        .any(|t| ct.contains(t));
    !textual
}

async fn new_body_file() -> Result<(tokio::fs::File, std::path::PathBuf)> {
    let path = crate::storage::bodies_dir()?.join(format!("{}.bin", uuid::Uuid::new_v4()));
    let file = tokio::fs::File::create(&path)
        .await
        .with_context(|| format!("failed to create {}", path.display()))?;
    Ok((file, path))
}

/// Snapshot of a built request: its own headers, plus the ones the client adds when
/// sending — reqwest's default `Accept: */*` (if not set), and `Host` /
/// `Content-Length` from the HTTP layer.
fn describe(
    request: &reqwest::Request,
    body: ActualBody,
    env: Option<&Environment>,
) -> ActualRequest {
    let headers: Vec<(String, String)> = request
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let has = |name: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name));
    let mut client_headers = Vec::new();
    if !has("accept") {
        client_headers.push(("accept".to_string(), "*/*".to_string()));
    }
    if !has("host")
        && let Some(host) = request.url().host_str()
    {
        let host = match request.url().port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        };
        client_headers.push(("host".to_string(), host));
    }
    if !has("content-length")
        && let Some(len) = request.body().and_then(|b| b.as_bytes()).map(<[u8]>::len)
    {
        client_headers.push(("content-length".to_string(), len.to_string()));
    }
    ActualRequest {
        at: chrono::Local::now(),
        method: request.method().to_string(),
        url: request.url().to_string(),
        headers,
        client_headers,
        body,
        environment: env.map(|e| e.name.clone()),
    }
}

/// Attach the body for the request's body mode, and describe it for `describe`.
async fn apply_body(
    builder: reqwest::RequestBuilder,
    req: &RequestItem,
    resolve: &impl Fn(&str) -> String,
) -> Result<(reqwest::RequestBuilder, ActualBody)> {
    match req.body_mode {
        BodyMode::Raw => {
            if req.body.is_empty() {
                Ok((builder, ActualBody::None))
            } else {
                let body = resolve(&req.body);
                Ok((builder.body(body.clone()), ActualBody::Text(body)))
            }
        }
        BodyMode::FormData => {
            let mut form = reqwest::multipart::Form::new();
            let mut parts = Vec::new();
            for field in &req.form_fields {
                if !field.enabled || field.key.is_empty() {
                    continue;
                }
                let key = resolve(&field.key);
                form = match &field.value {
                    FormValue::Text(v) => {
                        let value = resolve(v);
                        parts.push(format!("{key} = {value}"));
                        form.text(key, value)
                    }
                    FormValue::File(path) => {
                        let path = resolve(path);
                        let bytes = tokio::fs::read(&path).await.with_context(|| {
                            format!("failed to read file '{path}' for form field '{key}'")
                        })?;
                        let file_name = std::path::Path::new(&path)
                            .file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "file".to_string());
                        parts.push(format!(
                            "{key} = @{path} ({} bytes, filename \"{file_name}\")",
                            bytes.len()
                        ));
                        form.part(
                            key,
                            reqwest::multipart::Part::bytes(bytes).file_name(file_name),
                        )
                    }
                };
            }
            Ok((builder.multipart(form), ActualBody::Multipart(parts)))
        }
        BodyMode::Binary => {
            if req.binary_file.is_empty() {
                Ok((builder, ActualBody::None))
            } else {
                let path = resolve(&req.binary_file);
                let bytes = tokio::fs::read(&path)
                    .await
                    .with_context(|| format!("failed to read binary file '{path}'"))?;
                let size = bytes.len() as u64;
                Ok((builder.body(bytes), ActualBody::File { path, size }))
            }
        }
    }
}

#[cfg(test)]
// The storage guard is held across awaits on purpose: it serializes tests that use
// the data dir, and #[tokio::test] runs on a single-threaded runtime.
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    /// Serve one canned HTTP response on a local port; returns the URL.
    async fn serve_once(content_type: &str, extra: &str, body: Vec<u8>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\n{extra}content-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await;
            sock.write_all(head.as_bytes()).await.unwrap();
            sock.write_all(&body).await.unwrap();
        });
        format!("http://{addr}/file")
    }

    fn get(url: String) -> RequestItem {
        let mut req = RequestItem::new("t");
        req.url = url;
        req
    }

    #[tokio::test]
    async fn binary_bodies_are_streamed_to_a_file_byte_for_byte() {
        let _storage = crate::test_support::isolated_storage();
        let bytes: Vec<u8> = (0..=255u8).cycle().take(200_000).collect();
        let url = serve_once(
            "application/zip",
            "content-disposition: attachment; filename=\"a.zip\"\r\n",
            bytes.clone(),
        )
        .await;
        let resp = send_request(&get(url), None).await.result.unwrap();
        assert!(resp.binary && resp.body.is_empty());
        assert_eq!(resp.size, bytes.len() as u64);
        assert_eq!(std::fs::read(resp.body_file.unwrap()).unwrap(), bytes);
    }

    #[tokio::test]
    async fn text_that_is_not_utf8_falls_back_to_binary() {
        let _storage = crate::test_support::isolated_storage();
        let url = serve_once("text/plain", "", vec![b'o', b'k', 0xff, 0xfe]).await;
        let resp = send_request(&get(url), None).await.result.unwrap();
        assert!(resp.binary);
        assert_eq!(
            std::fs::read(resp.body_file.unwrap()).unwrap(),
            vec![b'o', b'k', 0xff, 0xfe]
        );
    }

    #[tokio::test]
    async fn json_stays_text_in_memory() {
        let _storage = crate::test_support::isolated_storage();
        let url = serve_once("application/json", "", br#"{"ok":true}"#.to_vec()).await;
        let resp = send_request(&get(url), None).await.result.unwrap();
        assert!(!resp.binary && resp.body_file.is_none());
        assert_eq!(resp.body, r#"{"ok":true}"#);
    }

    #[tokio::test]
    async fn actual_request_shows_what_went_on_the_wire() {
        let _storage = crate::test_support::isolated_storage();
        let url = serve_once("application/json", "", b"{}".to_vec()).await; // http://127.0.0.1:PORT/file
        let base = url.trim_end_matches("/file").to_string();
        let mut env = Environment::new("dev");
        env.variables.push(("base".into(), base.clone()));
        env.variables.push(("token".into(), "s3cret".into()));
        let mut req = RequestItem::new("t");
        req.method = crate::model::Method::Post;
        req.url = "{{base}}/:id".into();
        req.path_params = vec![("id".into(), "file".into())];
        req.headers.push(crate::model::Header::new(
            "Authorization",
            "Bearer {{token}}",
        ));
        req.body = r#"{"who":"{{token}}"}"#.into();

        let outcome = send_request(&req, Some(&env)).await;
        assert!(outcome.result.is_ok());
        let actual = outcome.actual.expect("snapshot");
        assert_eq!(actual.method, "POST");
        assert_eq!(
            actual.url,
            format!("{base}/file"),
            "variables and path params resolved"
        );
        assert!(
            actual
                .headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer s3cret")
        );
        let client: Vec<&str> = actual
            .client_headers
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        assert_eq!(client, vec!["accept", "host", "content-length"]);
        assert!(matches!(&actual.body, ActualBody::Text(b) if b == r#"{"who":"s3cret"}"#));
        assert_eq!(actual.environment.as_deref(), Some("dev"));
        let text = actual.to_text();
        assert!(text.starts_with(&format!("POST {base}/file\n")));
        assert!(text.contains("accept: */*    ← added by the HTTP client"));
    }

    #[tokio::test]
    async fn multipart_parts_and_failed_sends_are_described() {
        let _storage = crate::test_support::isolated_storage();
        // A port with nothing listening: the send fails, but the request was built.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut req = get(format!("http://127.0.0.1:{port}/upload"));
        req.method = crate::model::Method::Post;
        req.body_mode = BodyMode::FormData;
        req.form_fields
            .push(crate::model::FormField::new_text("note", "hello"));

        let outcome = send_request(&req, None).await;
        assert!(outcome.result.is_err(), "nothing is listening");
        let actual = outcome
            .actual
            .expect("built before sending, so still described");
        assert!(
            actual.headers.iter().any(
                |(k, v)| k == "content-type" && v.starts_with("multipart/form-data; boundary=")
            )
        );
        assert!(
            matches!(&actual.body, ActualBody::Multipart(parts) if parts == &vec!["note = hello".to_string()])
        );
    }
}
