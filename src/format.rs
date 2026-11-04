use serde_json::Value;

pub fn pretty_json(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    serde_json::to_string_pretty(&value).ok()
}
