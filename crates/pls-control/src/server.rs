//! Logique de session du canal de contrôle, indépendante du transport.
//!
//! [`ControlServer`] détient l'engine, la politique de session et le
//! token ; [`ControlSession`] détient l'état d'une connexion (handshake
//! réussi oui/non). Un message entrant donne au plus une réponse : un id
//! est exigé à l'analyse, donc une ligne produite à chaque appel.
//!
//! Cartographie des erreurs (voir `docs/control-protocol.md`) :
//! - `PLS-E-4xxx` (4001 token, 4002 version, 4003 inconnu, 4004 enveloppe,
//!   4006 permission) : incidents du CANAL, tranchés avant l'engine ;
//! - les codes `PLS-E-1xxx` restent tels quels : ce sont des échecs de la
//!   commande elle-même (`PLS-E-1003` peut donc signifier « session de
//!   run inconnue », pas « canal cassé »).
//!
//! L'audit journalise méthode, issue et durée — jamais les paramètres :
//! il vaut mieux ne rien tracer que redacted à moitié pendant l'UF.

use std::{path::PathBuf, sync::Arc, time::Instant};

use pls_base::{CommandCtx, CommandId, DangerLevel, ErrorCode, PlsError, Result, TrustLevel};
use pls_engine::Engine;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::PROTOCOL;
use crate::proto::{IncomingRequest, JsonResponse, parse_request};
use crate::token::ControlToken;

/// Politique d'une session : workspace exécuté et niveau de confiance.
///
/// `restricted` bloque toute commande `write`/`destructive` au canal
/// (`PLS-E-4006`) ; c'est le défaut. `trusted` laisse passer tout le
/// registre, comme un utilisateur local.
#[derive(Debug, Clone)]
pub struct SessionPolicy {
    pub workspace: PathBuf,
    pub trust: TrustLevel,
}

impl SessionPolicy {
    /// Workspace courant, confiance restreinte : le posture par défaut.
    pub fn restricted() -> Self {
        Self {
            workspace: PathBuf::from("."),
            trust: TrustLevel::Restricted,
        }
    }
}

/// Le serveur : engine + token + politique, sans état de connexion.
pub struct ControlServer {
    engine: Arc<Engine>,
    token: ControlToken,
    policy: SessionPolicy,
}

impl ControlServer {
    pub fn new(engine: Arc<Engine>, policy: SessionPolicy, token: ControlToken) -> Self {
        Self {
            engine,
            token,
            policy,
        }
    }

    pub fn policy(&self) -> &SessionPolicy {
        &self.policy
    }

    pub fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    /// Exécute une commande du registre comme le ferait l'UI, en teintant
    /// le canal de la politique de session (gate, job, annulation).
    async fn execute(&self, name: &str, params: &Value) -> Result<Value> {
        let command = CommandId::from(name);
        let meta = self
            .engine
            .commands()
            .into_iter()
            .find(|registered| registered.id == command)
            .ok_or_else(|| unknown_command(name))?;

        self.guard(meta.danger, name)?;

        let ctx = CommandCtx::new(
            self.engine.next_job(),
            self.policy.workspace.clone(),
            self.policy.trust,
        );
        // null → objet vide : `engine.execute` sans params vaut `{}`.
        let params = if params.is_null() {
            json!({})
        } else {
            params.clone()
        };

        let output = self.engine.execute(&command, &ctx, params).await?;

        Ok(json!({
            "job_id": ctx.job_id.get(),
            "status": "completed",
            "value": output.value,
        }))
    }

    /// Gate de la politique : `restricted` ne bloque que l'écriture.
    ///
    /// Ce gate est un filet de la session contrôle ; l'engine garde la
    /// sienne (MCP/source) sous ses propres règles.
    fn guard(&self, danger: DangerLevel, name: &str) -> Result<()> {
        if self.policy.trust.is_trusted() {
            return Ok(());
        }

        match danger {
            DangerLevel::Read => Ok(()),
            DangerLevel::Write | DangerLevel::Destructive => Err(PlsError::app(
                ErrorCode::CONTROL_PERMISSION,
                format!("command `{name}` needs write access, session is restricted"),
            )
            .with_remediation(
                "restart the server with `--trust trusted`, or use read-only commands",
            )),
        }
    }

    #[allow(clippy::unused_self)]
    fn not_implemented(&self, message: &str, remediation: &str) -> PlsError {
        PlsError::app(ErrorCode::NOT_IMPLEMENTED, message).with_remediation(remediation)
    }

    /// Traite une requête déjà authentifiée (ou pas encore).
    async fn handle(&self, request: IncomingRequest) -> Result<Value> {
        match request.method.as_str() {
            "hello" => self.hello(&request.params),
            "engine.execute" => {
                let params: ExecuteParams = parse_protocol_params(&request.params)?;
                let payload = params.params.unwrap_or_else(|| json!({}));
                self.execute(&params.command, &payload).await
            }
            "engine.cancel" => {
                let params = copy_params(&request.params);
                self.execute("engine.cancel", &params).await
            }
            "engine.jobs" => self.execute("engine.jobs", &json!({})).await,
            "commands.list" => {
                let commands = self.engine.commands();
                let value = serde_json::to_value(&commands).map_err(|error| {
                    PlsError::app(
                        ErrorCode::COMMAND,
                        format!("cannot serialize the command registry: {error}"),
                    )
                })?;
                Ok(json!({ "commands": value }))
            }
            "events.subscribe" => Err(self.not_implemented(
                "the control channel cannot stream events yet",
                "poll `engine.jobs` or `run.log`; streaming arrives with the event bus",
            )),
            other => Err(PlsError::app(
                ErrorCode::CONTROL_UNKNOWN,
                format!("unknown protocol method `{other}`"),
            )
            .with_remediation("hello, engine.execute, engine.cancel, engine.jobs, commands.list")),
        }
    }

    fn hello(&self, params: &Value) -> Result<Value> {
        let hello: HelloParams = parse_protocol_params(params)?;

        if hello.protocol != PROTOCOL {
            return Err(PlsError::app(
                ErrorCode::CONTROL_VERSION,
                format!(
                    "protocol `{}` unsupported, this server speaks {PROTOCOL}",
                    hello.protocol
                ),
            )
            .with_remediation(format!("negotiate `{PROTOCOL}`")));
        }

        if !self.token.verify(&hello.token) {
            return Err(
                PlsError::app(ErrorCode::CONTROL_TOKEN, "invalid session token").with_remediation(
                    "read the current token from the token file or server output",
                ),
            );
        }

        Ok(json!({
            "protocol": PROTOCOL,
            "server": format!("plstudio/{}", env!("CARGO_PKG_VERSION")),
            "workspace": self.policy.workspace.to_string_lossy(),
            "trust": trust_string(self.policy.trust),
            "commands": self.engine.command_count(),
        }))
    }
}

/// État d'une connexion : la porte hello ouvre les autres méthodes.
pub struct ControlSession<'server> {
    server: &'server ControlServer,
    authorized: bool,
}

impl<'server> ControlSession<'server> {
    pub fn new(server: &'server ControlServer) -> Self {
        Self {
            server,
            authorized: false,
        }
    }

    pub fn authorized(&self) -> bool {
        self.authorized
    }

    /// Un message entrant → la réponse à écrire (toujours citée : l'id est
    /// exigé à l'analyse). Journalise la méthode, l'issue et la durée.
    pub async fn dispatch(&mut self, line: &str) -> Option<JsonResponse> {
        let started = Instant::now();

        if line.len() > crate::proto::MAX_LINE_BYTES {
            tracing::warn!(
                target: "control",
                "rejecting a {}-byte line (limit {})",
                line.len(),
                crate::proto::MAX_LINE_BYTES
            );
            return Some(JsonResponse::from_pls(
                Value::Null,
                &PlsError::app(
                    ErrorCode::CONTROL_ENVELOPE,
                    format!(
                        "request line exceeds {} bytes",
                        crate::proto::MAX_LINE_BYTES
                    ),
                )
                .with_remediation("send smaller payloads in one object per line"),
            ));
        }

        match parse_request(line) {
            Err(envelope_error) => Some(JsonResponse::from_pls(Value::Null, &envelope_error)),
            Ok(request) => {
                let method = request.method.clone();
                let id = request.id.clone();

                let outcome = self.route(request).await;
                match &outcome {
                    Ok(_) => self.audit_ok(&method, started),
                    Err(error) => self.audit_error(&method, error, started),
                }
                match outcome {
                    Ok(result) => {
                        if method == "hello" {
                            self.authorized = true;
                        }
                        Some(JsonResponse::ok(id, result))
                    }
                    Err(error) => Some(JsonResponse::from_pls(id, &error)),
                }
            }
        }
    }

    /// Route avec la barrière du handshake : seul `hello` passe avant.
    async fn route(&mut self, request: IncomingRequest) -> Result<Value> {
        if !self.authorized && request.method != "hello" {
            return Err(PlsError::app(
                ErrorCode::CONTROL_PERMISSION,
                format!(
                    "method `{}` needs the `hello` handshake first",
                    request.method
                ),
            )
            .with_remediation("send hello with the session token"));
        }

        self.server.handle(request).await
    }

    fn audit_ok(&self, method: &str, started: Instant) {
        tracing::info!(
            target: "control",
            "method={method} outcome=ok elapsed_ms={} authorized={}",
            started.elapsed().as_millis(),
            self.authorized
        );
    }

    fn audit_error(&self, method: &str, error: &PlsError, started: Instant) {
        tracing::warn!(
            target: "control",
            "method={method} outcome=error code={} elapsed_ms={}",
            error.code(),
            started.elapsed().as_millis()
        );
    }
}

#[derive(Debug, Deserialize)]
struct HelloParams {
    protocol: String,
    token: String,
}

#[derive(Debug, Deserialize)]
struct ExecuteParams {
    command: String,
    /// Reproduit le protocole documenté (`wait`, `request_id` acceptés,
    /// ignorés en v0.1 synchrone) sans brider l'évolution du canal.
    #[serde(default)]
    params: Option<Value>,
}

/// Absence ou `null` de `params` → déserialisation directe ; une forme
/// non conforme donne `PLS-E-4004` (c'est le canal, pas une commande).
fn parse_protocol_params<T: serde::de::DeserializeOwned>(params: &Value) -> Result<T> {
    serde_json::from_value::<T>(params.clone()).map_err(|error| {
        PlsError::app(
            ErrorCode::CONTROL_ENVELOPE,
            format!("malformed request params: {error}"),
        )
        .with_remediation("check the documented shape of the method params")
    })
}

fn copy_params(params: &Value) -> Value {
    if params.is_null() {
        json!({})
    } else {
        params.clone()
    }
}

fn unknown_command(name: &str) -> PlsError {
    PlsError::app(
        ErrorCode::CONTROL_UNKNOWN,
        format!("unknown command `{name}`"),
    )
    .with_remediation("see `commands.list` for the registered commands")
}

fn trust_string(trust: TrustLevel) -> &'static str {
    if trust.is_trusted() {
        "trusted"
    } else {
        "restricted"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn server_with_trust(trust: TrustLevel) -> (ControlServer, ControlToken) {
        let token = ControlToken::generate().expect("randomness available");
        let server = ControlServer::new(
            Arc::new(Engine::new()),
            SessionPolicy {
                workspace: PathBuf::from("."),
                trust,
            },
            token.clone(),
        );
        (server, token)
    }

    fn line(method: &str, extra: &Value) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": extra,
        })
        .to_string()
    }

    fn reply_value(reply: Option<JsonResponse>) -> Value {
        let line = reply
            .expect("an id is required, so a reply always exists")
            .to_line();
        serde_json::from_str(line.trim_end()).expect("reply is valid json")
    }

    fn error_code(reply: &Value) -> i64 {
        reply
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    }

    fn error_data_code(reply: &Value) -> String {
        reply["error"]["data"]["code"]
            .as_str()
            .expect("data.code is the canonical string")
            .to_string()
    }

    async fn hello_ok(session: &mut ControlSession<'_>, token: &ControlToken) -> Value {
        let hello = line(
            "hello",
            &json!({
                "protocol": PROTOCOL,
                "token": token.as_hex(),
                "client": "tests/0",
            }),
        );
        let reply = reply_value(session.dispatch(&hello).await);
        assert!(reply.get("result").is_some(), "hello must succeed");
        assert!(session.authorized());
        reply
    }

    #[tokio::test]
    async fn hello_exposes_capabilities() {
        let token = ControlToken::generate().expect("generate");
        let server = ControlServer::new(
            Arc::new(Engine::new()),
            SessionPolicy {
                workspace: PathBuf::from("C:/src/demo"),
                trust: TrustLevel::Trusted,
            },
            token.clone(),
        );
        let mut session = ControlSession::new(&server);

        let reply = hello_ok(&mut session, &token).await;
        let result = reply.get("result").expect("result");
        assert_eq!(
            result.get("protocol").and_then(Value::as_str),
            Some(PROTOCOL)
        );
        assert_eq!(
            result.get("workspace").and_then(Value::as_str),
            Some("C:/src/demo")
        );
        assert_eq!(result.get("trust").and_then(Value::as_str), Some("trusted"));
        // 25 : le registre de `Engine::new` ; les tests lib ajoutent le
        // leur (test.echo) après coup, ce qui montait à 26 chez eux.
        assert_eq!(result.get("commands").and_then(Value::as_u64), Some(25));
    }

    #[tokio::test]
    async fn hello_rejects_the_wrong_token_and_stays_locked() {
        let (server, _token) = server_with_trust(TrustLevel::Trusted);
        let mut session = ControlSession::new(&server);

        let hello = line(
            "hello",
            &json!({ "protocol": PROTOCOL, "token": "f".repeat(64) }),
        );
        let reply = reply_value(session.dispatch(&hello).await);
        assert_eq!(error_code(&reply), 4001);
        assert_eq!(error_data_code(&reply), "PLS-E-4001");

        assert!(!session.authorized(), "a failed hello does not open");
    }

    #[tokio::test]
    async fn hello_rejects_an_unknown_protocol_version() {
        let (server, token) = server_with_trust(TrustLevel::Trusted);
        let mut session = ControlSession::new(&server);

        let hello = line(
            "hello",
            &json!({ "protocol": "plstudio-control/9", "token": token.as_hex() }),
        );
        let reply = reply_value(session.dispatch(&hello).await);
        assert_eq!(error_code(&reply), 4002);
    }

    #[tokio::test]
    async fn methods_need_the_handshake_first() {
        let (server, _token) = server_with_trust(TrustLevel::Trusted);
        let mut session = ControlSession::new(&server);

        let reply = reply_value(session.dispatch(&line("engine.jobs", &json!({}))).await);
        assert_eq!(error_code(&reply), 4006);
        assert_eq!(error_data_code(&reply), "PLS-E-4006");
    }

    #[tokio::test]
    async fn execute_runs_a_registered_read_command() {
        let (server, token) = server_with_trust(TrustLevel::Trusted);
        let mut session = ControlSession::new(&server);
        hello_ok(&mut session, &token).await;

        let request = line(
            "engine.execute",
            &json!({ "command": "editor.list", "wait": true, "request_id": "r-1" }),
        );
        let reply = reply_value(session.dispatch(&request).await);
        let result = reply.get("result").expect("result");
        assert_eq!(
            result.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert!(result.get("job_id").is_some());
        assert!(
            result
                .get("value")
                .and_then(|value| value.get("buffers"))
                .and_then(Value::as_array)
                .is_some()
        );

        // Un id valide avec des params de méthode invalides échoÎ quand
        // même : la barrière enveloppe est passée, l'echec vient de la
        // méthode (`command` absent → 4004, pas d'engine appelé).
        let reply = reply_value(
            session
                .dispatch(r#"{"jsonrpc":"2.0","id":9,"method":"engine.execute","params":{}}"#)
                .await,
        );
        assert_eq!(reply.get("id"), Some(&json!(9)));
        assert_eq!(error_code(&reply), 4004);
    }

    #[tokio::test]
    async fn unknown_command_and_unknown_method_say_4003() {
        let (server, token) = server_with_trust(TrustLevel::Trusted);
        let mut session = ControlSession::new(&server);
        hello_ok(&mut session, &token).await;

        let reply = reply_value(
            session
                .dispatch(&line("engine.execute", &json!({ "command": "nope.fake" })))
                .await,
        );
        assert_eq!(error_code(&reply), 4003);

        let reply = reply_value(session.dispatch(&line("bogus.method", &json!({}))).await);
        assert_eq!(error_code(&reply), 4003);
    }

    #[tokio::test]
    async fn malformed_lines_answer_with_a_null_id() {
        let (server, _token) = server_with_trust(TrustLevel::Trusted);
        let mut session = ControlSession::new(&server);

        let reply = reply_value(session.dispatch("not a json line").await);
        assert_eq!(reply.get("id"), Some(&Value::Null));
        assert_eq!(error_code(&reply), 4004);

        // Une enveloppe non conforme (jsonrpc erroné) ne fait pas echo de
        // l'id : seule une ligne PARSÉE l'échoÎ — id null ici.
        let reply = reply_value(session.dispatch(r#"{"jsonrpc":"1.0","id":3}"#).await);
        assert_eq!(reply.get("id"), Some(&Value::Null));
        assert_eq!(error_code(&reply), 4004);
    }

    #[tokio::test]
    async fn restricted_sessions_cannot_run_write_commands() {
        let (server, token) = server_with_trust(TrustLevel::Restricted);
        let mut session = ControlSession::new(&server);
        hello_ok(&mut session, &token).await;

        let reply = reply_value(
            session
                .dispatch(&line(
                    "engine.execute",
                    &json!({ "command": "engine.cancel" }),
                ))
                .await,
        );
        assert_eq!(error_code(&reply), 4006);
        assert_eq!(error_data_code(&reply), "PLS-E-4006");
    }

    #[tokio::test]
    async fn trusted_sessions_reach_the_command_and_its_own_errors() {
        let (server, token) = server_with_trust(TrustLevel::Trusted);
        let mut session = ControlSession::new(&server);
        hello_ok(&mut session, &token).await;

        // engine.cancel est Write : sous Trusted, la demande ATTEINT la
        // commande, qui refuse pour son propre motif (job inconnu, 1003).
        let request = line(
            "engine.execute",
            &json!({ "command": "engine.cancel", "params": { "job_id": 999 } }),
        );
        let reply = reply_value(session.dispatch(&request).await);
        assert_eq!(error_code(&reply), 1003);
        assert_eq!(error_data_code(&reply), "PLS-E-1003");
    }

    #[tokio::test]
    async fn events_subscribe_is_a_documented_gap() {
        let (server, token) = server_with_trust(TrustLevel::Trusted);
        let mut session = ControlSession::new(&server);
        hello_ok(&mut session, &token).await;

        let reply = reply_value(
            session
                .dispatch(&line("events.subscribe", &json!({})))
                .await,
        );
        assert_eq!(error_code(&reply), 1005);
        assert_eq!(error_data_code(&reply), "PLS-E-1005");
    }

    #[tokio::test]
    async fn engine_jobs_method_lists_journal() {
        let (server, token) = server_with_trust(TrustLevel::Trusted);
        let mut session = ControlSession::new(&server);
        hello_ok(&mut session, &token).await;

        let reply = reply_value(session.dispatch(&line("engine.jobs", &json!({}))).await);
        let result = reply.get("result").expect("result");
        assert!(
            result
                .get("value")
                .and_then(|value| value.get("jobs"))
                .and_then(Value::as_array)
                .is_some()
        );
    }
}
