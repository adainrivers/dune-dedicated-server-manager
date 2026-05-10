use serde_json::Value;

pub fn redact_text(input: &str) -> String {
    let mut output = Vec::new();
    for line in input.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("token")
            || lower.contains("secret")
            || lower.contains("password")
            || lower.contains("apikey")
            || lower.contains("api_key")
            || lower.contains("serviceauth")
            || looks_like_jwt(line.trim())
        {
            output.push("<redacted>".to_string());
        } else {
            output.push(line.to_string());
        }
    }
    output.join("\n")
}

pub fn redact_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if is_sensitive_key(key) {
                    *child = Value::String("<redacted>".to_string());
                } else {
                    redact_json(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_json(item);
            }
        }
        Value::String(text) if looks_like_jwt(text) => {
            *text = "<redacted>".to_string();
        }
        Value::String(text) => {
            if text.contains("ServiceAuthToken=") {
                *text = "<redacted>".to_string();
            }
        }
        _ => {}
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("apikey")
        || lower.contains("api_key")
        || lower.contains("auth")
        || lower == "key"
}

fn looks_like_jwt(value: &str) -> bool {
    let mut parts = value.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(a), Some(b), Some(c), None) if a.len() > 8 && b.len() > 8 && c.len() > 8
    )
}
