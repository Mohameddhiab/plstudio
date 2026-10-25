//! Enveloppe JSON-RPC 2.0 du canal de contrôle (sous-ensemble strict).
//!
//! Une ligne = un objet : `{"jsonrpc":"2.0","id":...,"method":...,"params":...}`.
//! L'`id` est obligatoire (pas de notifications entrantes en v0.1) et la
//! réponse l'échoÎ tel quel. Les erreurs d'enveloppe (JSON invalide,
//! `jsonrpc` absent/erroné, ligne surdimensionnée) utilisent
//! `PLS-E-4004` ; les erreurs de méthode ou de commande inconnues,
//! `PLS-E-4003`.
//!
//! Le champ `jsonrpc` est exigé égal à `"2.0"`, mais la version du
//! protocole plstudio elle-même vit dans le handshake `hello`
//! (`PLS-E-4002`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use pls_base::{ErrorCode, PlsError, Result};

/// Limite d'une ligne de requête : au-delà, c'est un abus du canal.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// Une requête entrante valide.
#[derive(Debug, Clone)]
pub struct IncomingRequest {
    pub id: Value,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Deserialize)]
struct RawRequest {
    jsonrpc: String,
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

/// Parse une ligne en requête.
///
/// L'`id` manquant n'est pas toléré : sans id, aucune réponse utile n'est
/// possible, la ligne est rejetée (`PLS-E-4004`) et répond par un id null.
pub fn parse_request(line: &str) -> Result<IncomingRequest> {
    const WHERE: &str = "expected one object per line: {\"jsonrpc\":\"2.0\",\"id\":...,\"method\":...,\"params\":...}";

    let raw: RawRequest = serde_json::from_str(line).map_err(|error| {
        PlsError::app(
            ErrorCode::CONTROL_ENVELOPE,
            format!("malformed request line: {error}"),
        )
        .with_remediation(WHERE)
    })?;

    if raw.jsonrpc != "2.0" {
        return Err(PlsError::app(
            ErrorCode::CONTROL_ENVELOPE,
            format!("unsupported jsonrpc field `{}`", raw.jsonrpc),
        )
        .with_remediation(WHERE));
    }

    if !raw.params.is_object() && !raw.params.is_null() {
        return Err(PlsError::app(
            ErrorCode::CONTROL_ENVELOPE,
            "params must be an object when present",
        )
        .with_remediation(WHERE));
    }

    Ok(IncomingRequest {
        id: raw.id,
        method: raw.method,
        params: raw.params,
    })
}

/// Réponse JSON-RPC : `result` ou `error`, jamais les deux.
#[derive(Debug, Serialize)]
pub struct JsonResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonError>,
}

#[derive(Debug, Serialize)]
pub struct JsonError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonError {
    /// Construit l'erreur depuis une `PlsError` : le code numérique JSON-RPC
    /// est le suffixe du code stable (`PLS-E-1003` → `1003`), la chaîne en
    /// reste la référence canonique dans `data.code`.
    pub fn from_pls(error: &PlsError) -> Self {
        let (message, remediation) = match error {
            PlsError::App {
                message,
                remediation,
                ..
            }
            | PlsError::Config {
                message,
                remediation,
                ..
            } => (message.clone(), remediation.clone()),
            PlsError::Io { source, .. } => (format!("I/O error: {source}"), None),
        };

        let number: i64 = error
            .code()
            .as_str()
            .strip_prefix("PLS-E-")
            .and_then(|suffix| suffix.parse().ok())
            .unwrap_or(-1);

        let mut data = serde_json::json!({ "code": error.code().as_str() });
        if let Some(remediation) = remediation {
            data["remediation"] = serde_json::json!(remediation);
        }

        Self {
            code: number,
            message,
            data: Some(data),
        }
    }
}

impl JsonResponse {
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Value, error: JsonError) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(error),
        }
    }

    pub fn from_pls(id: Value, error: &PlsError) -> Self {
        Self::error(id, JsonError::from_pls(error))
    }

    /// Ligne complète (JSON + `\n`), prête à être écrite sur le canal.
    pub fn to_line(&self) -> String {
        match serde_json::to_string(self) {
            Ok(payload) => format!("{payload}\n"),
            // La sérialisation d'une structure plate ne peut pas échouer :
            // le repli garde le canal vivant et documente l'incident.
            Err(error) => format!(
                "{{\"jsonrpc\":\"2.0\",\"error\":{{\"code\":-32700,\"message\":\"serialization failed: {error}\"}}}}\n"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_a_documented_request() {
        let request = parse_request(
            r#"{"jsonrpc":"2.0","id":2,"method":"engine.execute",
                "params":{"command":"sdk.detect","params":{},"wait":true,"request_id":"req-1"}}"#,
        )
        .expect("documented shape");

        assert_eq!(request.id, json!(2));
        assert_eq!(request.method, "engine.execute");
        assert_eq!(request.params.get("command"), Some(&json!("sdk.detect")));
    }

    #[test]
    fn requires_jsonrpc_two_point_zero_and_an_id() {
        let no_jsonrpc =
            parse_request(r#"{"id":1,"method":"engine.jobs"}"#).expect_err("no jsonrpc field");
        assert_eq!(no_jsonrpc.code(), ErrorCode::CONTROL_ENVELOPE);

        let wrong = parse_request(r#"{"jsonrpc":"1.0","id":1,"method":"engine.jobs"}"#)
            .expect_err("wrong jsonrpc");
        assert_eq!(wrong.code(), ErrorCode::CONTROL_ENVELOPE);

        let no_id =
            parse_request(r#"{"jsonrpc":"2.0","method":"engine.jobs"}"#).expect_err("no id");
        assert_eq!(no_id.code(), ErrorCode::CONTROL_ENVELOPE);
    }

    #[test]
    fn params_may_be_absent_but_not_a_scalar() {
        let absent = parse_request(r#"{"jsonrpc":"2.0","id":1,"method":"engine.jobs"}"#)
            .expect("absent params allowed");
        assert!(absent.params.is_null());

        let scalar = parse_request(r#"{"jsonrpc":"2.0","id":1,"method":"x","params":42}"#)
            .expect_err("scalar params");
        assert_eq!(scalar.code(), ErrorCode::CONTROL_ENVELOPE);
    }

    #[test]
    fn numeric_jsonrpc_code_comes_from_the_stable_suffix() {
        let error = PlsError::app(ErrorCode::CONTROL_TOKEN, "invalid session token");
        let mapped = JsonError::from_pls(&error);

        assert_eq!(mapped.code, 4001);
        assert!(mapped.message.contains("token"));
    }

    #[test]
    fn responses_carry_either_result_or_error() {
        let ok = JsonResponse::ok(json!(7), json!({ "status": "completed" }));
        let ok_text = ok.to_line();
        assert!(ok_text.starts_with(r#"{"jsonrpc":"2.0","id":7,"result":"#));
        assert!(!ok_text.contains("error"));

        let bad = JsonResponse::from_pls(
            Value::Null,
            &PlsError::app(ErrorCode::CONTROL_PERMISSION, "handshake required")
                .with_remediation("send `hello` first"),
        );
        let bad_value: Value =
            serde_json::from_str(bad.to_line().trim_end()).expect("error line parses");
        assert_eq!(bad_value.get("id"), Some(&Value::Null));
        assert_eq!(
            bad_value
                .get("error")
                .and_then(|error| error.get("code"))
                .and_then(Value::as_i64),
            Some(4006)
        );
        assert!(
            bad_value["error"]["data"]["code"]
                .as_str()
                .is_some_and(|code| code == "PLS-E-4006")
        );
    }
}
