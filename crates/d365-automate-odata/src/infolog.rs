//! Infolog / OData operation-status parser.
//!
//! Where SAP BAPIs return a `BAPIRET2` table, Dynamics 365 surfaces the
//! business-side outcome of an operation two ways:
//!   - an **OData error payload** — `{ "error": { "code", "message" } }` —
//!     on a 4xx/5xx response, and
//!   - the **Infolog** — the list of `Error`/`Warning`/`Info` messages the
//!     X++ runtime accumulates during the operation.
//!
//! Agents that look only at the JSON-RPC error object miss these structured
//! business messages. This helper turns either shape into a typed list that
//! tools can surface in their `CallToolResult`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum InfologSeverity {
    /// Operation reported success.
    Success,
    /// `Error` — the operation did NOT complete its work.
    Error,
    /// `Warning`.
    Warning,
    /// `Info`.
    Info,
    /// Anything else (forward-compat with future Dynamics 365 extensions).
    Unknown(char),
}

impl InfologSeverity {
    /// Parse a Dynamics 365 Infolog severity string (`"Error"`, `"Warning"`,
    /// `"Info"`) or a single severity char (`E`/`W`/`I`/`S`).
    pub fn parse(s: &str) -> Self {
        let t = s.trim();
        match t.to_ascii_lowercase().as_str() {
            "success" | "s" => Self::Success,
            "error" | "e" => Self::Error,
            "warning" | "warn" | "w" => Self::Warning,
            "info" | "information" | "i" => Self::Info,
            _ => Self::Unknown(t.chars().next().unwrap_or(' ')),
        }
    }

    /// Whether this severity indicates the operation failed (so the caller
    /// must NOT treat the `$batch` change set as committed).
    pub fn is_failure(self) -> bool {
        matches!(self, Self::Error | Self::Unknown(_))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfologMessage {
    pub severity: InfologSeverity,
    /// OData error code or X++ exception code (e.g. `"LedgerJournalCheck"`).
    pub code: String,
    pub text: String,
    /// Offending field / OData property, if the message names one.
    pub field: Option<String>,
    /// Originating environment, if reported.
    pub environment: Option<String>,
}

impl InfologMessage {
    pub fn is_failure(&self) -> bool { self.severity.is_failure() }
}

/// Parse a JSON value into Infolog messages.  Accepts:
///   - an OData error payload `{ "error": { "code", "message" } }`
///   - an array of Infolog rows (`[{ "severity", "message" }, ...]`)
///   - an `outputs.Infolog` / `outputs.RETURN` slot (the mock output shape)
///
/// Returns an empty list if no recognised shape is found.
pub fn parse_infolog(value: &serde_json::Value) -> Vec<InfologMessage> {
    let mut out = Vec::new();
    walk(value, &mut out, 0);
    out
}

fn walk(v: &serde_json::Value, out: &mut Vec<InfologMessage>, depth: usize) {
    if depth > 8 { return; }
    match v {
        serde_json::Value::Object(map) => {
            // OData error payload: { "error": { "code", "message" } }.
            if let Some(err) = map.get("error").and_then(|e| e.as_object()) {
                let code = first_str(err, &["code", "Code"]).unwrap_or_default();
                let text = err.get("message")
                    .map(odata_message_text)
                    .unwrap_or_default();
                out.push(InfologMessage {
                    severity: InfologSeverity::Error,
                    code,
                    text,
                    field: first_str(err, &["target", "Target"]),
                    environment: None,
                });
                return;
            }
            // Infolog row: has a severity and a message/text.
            let has_sev = map.contains_key("severity") || map.contains_key("Severity")
                || map.contains_key("type") || map.contains_key("TYPE");
            let has_msg = map.contains_key("message") || map.contains_key("Message")
                || map.contains_key("text") || map.contains_key("MESSAGE");
            if has_sev && has_msg {
                let sev = first_str(map, &["severity", "Severity", "type", "TYPE"])
                    .map(|s| InfologSeverity::parse(&s))
                    .unwrap_or(InfologSeverity::Unknown(' '));
                out.push(InfologMessage {
                    severity: sev,
                    code: first_str(map, &["code", "Code", "ID", "id"]).unwrap_or_default(),
                    text: first_str(map, &["message", "Message", "text", "MESSAGE"]).unwrap_or_default(),
                    field: first_str(map, &["field", "Field", "FIELD", "target"]),
                    environment: first_str(map, &["environment", "system", "SYSTEM"]),
                });
                return;
            }
            for v in map.values() { walk(v, out, depth + 1); }
        }
        serde_json::Value::Array(arr) => {
            for v in arr { walk(v, out, depth + 1); }
        }
        _ => {}
    }
}

/// OData wraps `message` either as a plain string or `{ "value": "..." }`.
fn odata_message_text(v: &serde_json::Value) -> String {
    if let Some(s) = v.as_str() { return s.to_string(); }
    if let Some(inner) = v.get("value").and_then(|x| x.as_str()) { return inner.to_string(); }
    v.to_string()
}

fn first_str(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(v) = obj.get(*k) {
            if let Some(s) = v.as_str() { return Some(s.to_string()); }
            if v.is_number() { return Some(v.to_string()); }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_array_of_infolog_rows() {
        let v = serde_json::json!([
            {"severity": "Error", "code": "LedgerPeriodClosed", "message": "Posting period is not open"},
            {"severity": "Info", "code": "LedgerPosted", "message": "Voucher posted"},
        ]);
        let parsed = parse_infolog(&v);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].severity, InfologSeverity::Error);
        assert!(parsed[0].is_failure());
        assert_eq!(parsed[1].severity, InfologSeverity::Info);
        assert!(!parsed[1].is_failure());
    }

    #[test]
    fn parses_odata_error_payload() {
        let v = serde_json::json!({
            "error": { "code": "Forbidden", "message": { "value": "Insufficient privileges" }, "target": "LedgerJournalTrans" }
        });
        let parsed = parse_infolog(&v);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].severity, InfologSeverity::Error);
        assert_eq!(parsed[0].text, "Insufficient privileges");
        assert_eq!(parsed[0].field.as_deref(), Some("LedgerJournalTrans"));
    }

    #[test]
    fn parses_nested_outputs_infolog() {
        let v = serde_json::json!({
            "executed_on": "gt-prod",
            "outputs": {
                "Infolog": [ {"severity": "Warning", "code": "DimDefaulted", "message": "Financial dimension defaulted"} ],
                "RecId": "5637144576"
            }
        });
        let parsed = parse_infolog(&v);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].text, "Financial dimension defaulted");
        assert_eq!(parsed[0].code, "DimDefaulted");
    }

    #[test]
    fn empty_value_returns_empty_list() {
        assert!(parse_infolog(&serde_json::Value::Null).is_empty());
        assert!(parse_infolog(&serde_json::json!({"unrelated": 1})).is_empty());
    }

    #[test]
    fn severity_is_failure_classification() {
        assert!(InfologSeverity::Error.is_failure());
        assert!(InfologSeverity::Unknown('Z').is_failure(),
            "Unknown severities must be treated as failures so unrecognised responses don't cause silent commits");
        assert!(!InfologSeverity::Success.is_failure());
        assert!(!InfologSeverity::Warning.is_failure());
        assert!(!InfologSeverity::Info.is_failure());
    }
}
