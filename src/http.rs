use anyhow::{Context, Result};
use std::time::Instant;

use crate::model::{Environment, RequestItem, ResponseData};

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

    if !req.body.is_empty() {
        builder = builder.body(resolve(&req.body));
    }

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
