use anyhow::{Context, Result};
use std::time::Instant;

use crate::model::{BodyMode, Environment, FormValue, RequestItem, ResponseData};

pub async fn send_request(req: &RequestItem, env: Option<&Environment>) -> Result<ResponseData> {
    let resolve = |s: &str| -> String {
        match env {
            Some(e) => e.resolve(s),
            None => s.to_string(),
        }
    };

    let url = req.resolve_url(&resolve(&req.url));
    if url::Url::parse(&url).is_err() {
        anyhow::bail!(
            "'{url}' is not a valid URL (missing scheme? try https://...) \
             — check for unresolved {{{{var}}}} placeholders (see the 'e' environment manager) \
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

    builder = apply_body(builder, req, &resolve).await?;

    let start = Instant::now();
    let resp = builder
        .send()
        .await
        .with_context(|| format!("{} {url} failed", req.method.as_str()))?;
    let status = resp.status().as_u16();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let body = resp
        .text()
        .await
        .context("request succeeded but reading the response body failed")?;
    let duration_ms = start.elapsed().as_millis();

    Ok(ResponseData {
        status,
        headers,
        body,
        duration_ms,
    })
}

/// Attach whichever body representation `req.body_mode` selects. `resolve`
/// substitutes `{{var}}` (and is also applied to file paths, so a path can
/// itself use an environment variable, e.g. `{{fixtures_dir}}/avatar.png`).
async fn apply_body(
    builder: reqwest::RequestBuilder,
    req: &RequestItem,
    resolve: &impl Fn(&str) -> String,
) -> Result<reqwest::RequestBuilder> {
    match req.body_mode {
        BodyMode::Raw => {
            if req.body.is_empty() {
                Ok(builder)
            } else {
                Ok(builder.body(resolve(&req.body)))
            }
        }
        BodyMode::FormData => {
            let mut form = reqwest::multipart::Form::new();
            for field in &req.form_fields {
                if !field.enabled || field.key.is_empty() {
                    continue;
                }
                let key = resolve(&field.key);
                form = match &field.value {
                    FormValue::Text(v) => form.text(key, resolve(v)),
                    FormValue::File(path) => {
                        let path = resolve(path);
                        let bytes = tokio::fs::read(&path)
                            .await
                            .with_context(|| format!("failed to read file '{path}' for form field '{key}'"))?;
                        let file_name = std::path::Path::new(&path)
                            .file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "file".to_string());
                        form.part(key, reqwest::multipart::Part::bytes(bytes).file_name(file_name))
                    }
                };
            }
            Ok(builder.multipart(form))
        }
        BodyMode::Binary => {
            if req.binary_file.is_empty() {
                Ok(builder)
            } else {
                let path = resolve(&req.binary_file);
                let bytes = tokio::fs::read(&path)
                    .await
                    .with_context(|| format!("failed to read binary file '{path}'"))?;
                Ok(builder.body(bytes))
            }
        }
    }
}
