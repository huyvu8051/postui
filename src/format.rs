use serde_json::Value;

pub fn pretty_json(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

/// Pretty-print JSON-looking text without parsing it, stopping once the output reaches
/// about `max_out` bytes. Used for the start of bodies too large to format fully:
/// it's linear in the output and allocates nothing but the result. Formatting follows
/// the brackets, commas and colons outside strings; invalid JSON just comes out oddly.
pub fn pretty_json_prefix(body: &str, max_out: usize) -> String {
    let mut out = String::with_capacity(max_out.min(body.len() * 2) + 64);
    let mut indent = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let newline = |out: &mut String, indent: usize| {
        out.push('\n');
        out.extend(std::iter::repeat_n(' ', indent * 2));
    };
    for c in body.chars() {
        if out.len() >= max_out {
            break;
        }
        if in_string {
            out.push(c);
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '{' | '[' => {
                out.push(c);
                indent += 1;
                newline(&mut out, indent);
            }
            '}' | ']' => {
                indent = indent.saturating_sub(1);
                newline(&mut out, indent);
                out.push(c);
            }
            ',' => {
                out.push(c);
                newline(&mut out, indent);
            }
            ':' => out.push_str(": "),
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_formats_like_pretty_json_for_complete_input() {
        let body = r#"{"a":[1,{"b":"x, {y}: \"z\""}],"c":null}"#;
        let ours = pretty_json_prefix(body, usize::MAX);
        let reparsed: serde_json::Value = serde_json::from_str(&ours).expect("still valid JSON");
        assert_eq!(
            reparsed,
            serde_json::from_str::<serde_json::Value>(body).unwrap()
        );
        assert!(
            ours.contains(r#""b": "x, {y}: \"z\"""#),
            "strings are untouched"
        );
        assert!(ours.lines().count() > 5);
    }

    #[test]
    fn prefix_stops_near_the_limit() {
        let body = format!("[{}0]", "1,".repeat(100_000));
        let out = pretty_json_prefix(&body, 10_000);
        assert!(out.len() >= 10_000 && out.len() < 10_100, "{}", out.len());
    }
}
