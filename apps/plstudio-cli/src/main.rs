//! CLI headless de plstudio.
//!
//! Toute action passe par le moteur : `run --cmd <id> --param '{...}'`
//! exécute une commande et renvoie son résultat JSON. `serve` héberge le
//! canal de contrôle longue durée (`plstudio-control/1`) sur TCP local ;
//! MCP arrive dans les phases suivantes.

#![forbid(unsafe_code)]

use std::{env, path::PathBuf, process::ExitCode, sync::Arc};

use pls_base::{CommandCtx, CommandId, ErrorCode, PlsError, TrustLevel};
use pls_control::{ControlServer, ControlToken, PROTOCOL, SessionPolicy, run_until_signal};
use pls_engine::Engine;
use serde_json::{Value, json};
use tokio::net::TcpListener;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("--version" | "version") => {
            println!("plstudio-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("commands") => {
            let as_json = args.iter().skip(1).any(|arg| arg == "--json");
            list_commands(as_json)
        }
        Some("run") => run_command(&args[1..]),
        Some("serve") => serve_command(&args[1..]),
        Some("--help" | "-h" | "help") | None => {
            print_help();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown subcommand: {other}");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn list_commands(as_json: bool) -> ExitCode {
    let engine = Engine::new();
    let commands = engine.commands();

    if as_json {
        let payload = match serde_json::to_string_pretty(&commands) {
            Ok(payload) => payload,
            Err(error) => {
                eprint_json(&error_payload(&PlsError::app(
                    ErrorCode::COMMAND,
                    format!("failed to serialize the command registry: {error}"),
                )));
                return ExitCode::FAILURE;
            }
        };
        println!("{payload}");
        return ExitCode::SUCCESS;
    }

    for meta in &commands {
        println!("{}\t{:?}\t{}", meta.id, meta.danger, meta.title);
    }

    ExitCode::SUCCESS
}

/// Spécification parsée de `run`.
#[derive(Debug)]
struct RunSpec {
    command: CommandId,
    params: Value,
    workspace: PathBuf,
    trust: TrustLevel,
}

/// `run --cmd <id> [--param {json}] [--workspace dir] [--trust level]`
fn run_command(args: &[String]) -> ExitCode {
    let spec = match parse_run_args(args) {
        Ok(spec) => spec,
        Err(usage_error) => {
            eprintln!("{usage_error}");
            print_run_usage();
            return ExitCode::from(2);
        }
    };
    let RunSpec {
        command,
        params,
        workspace,
        trust,
    } = spec;

    let engine = Engine::new();
    let ctx = CommandCtx::new(engine.next_job(), workspace, trust);

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprint_json(&error_payload(&PlsError::app(
                ErrorCode::COMMAND,
                format!("failed to start the async runtime: {error}"),
            )));
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(engine.execute(&command, &ctx, params)) {
        Ok(output) => {
            println!("{}", pretty(&output.value));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprint_json(&error_payload(&error));
            ExitCode::FAILURE
        }
    }
}

/// Spec parsée de `serve` : héberger le canal de contrôle TCP local.
#[derive(Debug)]
struct ServeSpec {
    port: u16,
    workspace: PathBuf,
    trust: TrustLevel,
    token_file: Option<PathBuf>,
}

/// `serve [--port n] [--workspace dir] [--trust level] [--token-file path]`
///
/// Le port par défaut est éphémère (0) ; la confiance par défaut est
/// `restricted` : contrairement à `run --trust trusted`, le canal sert
/// potentiellement un client distant (même en loopback), le filet
/// lecture-seule est donc le défaut documenté.
fn serve_command(args: &[String]) -> ExitCode {
    let spec = match parse_serve_args(args) {
        Ok(spec) => spec,
        Err(usage_error) => {
            eprintln!("{usage_error}");
            print_serve_usage();
            return ExitCode::from(2);
        }
    };
    let ServeSpec {
        port,
        workspace,
        trust,
        token_file,
    } = spec;

    // Audit du canal sur stderr (RUST_LOG pour l'affiner) ; la seule ligne
    // machine sur stdout reste l'annonce du serveur, sans aucun secret.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let token = match ControlToken::generate() {
        Ok(token) => token,
        Err(error) => {
            eprint_json(&error_payload(&error));
            return ExitCode::FAILURE;
        }
    };

    let token_path = token_file.unwrap_or_else(default_token_file);
    if let Err(error) = write_token_file(&token_path, token.as_hex()) {
        eprint_json(&error_payload(&PlsError::app(
            ErrorCode::CONFIG,
            format!("cannot write the session token file: {error}"),
        )));
        return ExitCode::FAILURE;
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprint_json(&error_payload(&PlsError::app(
                ErrorCode::COMMAND,
                format!("failed to start the async runtime: {error}"),
            )));
            return ExitCode::FAILURE;
        }
    };

    runtime.block_on(async move {
        let listener = match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => listener,
            Err(error) => {
                eprint_json(&error_payload(&PlsError::app(
                    ErrorCode::CONFIG,
                    format!("cannot bind the control channel on 127.0.0.1:{port}: {error}"),
                )));
                return ExitCode::FAILURE;
            }
        };
        let bound_port = listener
            .local_addr()
            .map(|addr| addr.port())
            .unwrap_or(port);

        // Une seule ligne JSON lisible par la machine : le fichier de token
        // porte le secret ; la sortie standard n'en contient aucun.
        println!(
            "{}",
            pretty(&json!({
                "protocol": PROTOCOL,
                "port": bound_port,
                "token_file": token_path.to_string_lossy(),
                "workspace": workspace.to_string_lossy(),
                "trust": if trust.is_trusted() { "trusted" } else { "restricted" },
            }))
        );

        let server = Arc::new(ControlServer::new(
            Arc::new(Engine::new()),
            SessionPolicy { workspace, trust },
            token,
        ));

        match run_until_signal(listener, server).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprint_json(&error_payload(&error));
                ExitCode::FAILURE
            }
        }
    })
}

/// Parse les arguments de `serve` ; usage en cas d'erreur.
fn parse_serve_args(args: &[String]) -> Result<ServeSpec, String> {
    let mut port = 0;
    let mut workspace = PathBuf::from(".");
    let mut trust = TrustLevel::Restricted;
    let mut token_file: Option<PathBuf> = None;

    let mut index = 0;
    while index < args.len() {
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("missing value after {}", args[index]))?;

        match args[index].as_str() {
            "--port" => {
                port = value
                    .parse::<u16>()
                    .map_err(|error| format!("--port expects a number 0..=65535: {error}"))?;
            }
            "--workspace" => workspace = PathBuf::from(value),
            "--trust" => {
                trust = match value.as_str() {
                    "trusted" => TrustLevel::Trusted,
                    "restricted" => TrustLevel::Restricted,
                    other => {
                        return Err(format!(
                            "unknown trust level: {other} (expected trusted|restricted)"
                        ));
                    }
                };
            }
            "--token-file" => token_file = Some(PathBuf::from(value)),
            other_flag => return Err(format!("unknown `serve` flag: {other_flag}")),
        }

        index += 2;
    }

    Ok(ServeSpec {
        port,
        workspace,
        trust,
        token_file,
    })
}

/// Chemin du fichier de token par défaut, hors projet :
/// `%LOCALAPPDATA%\plstudio\control\cli-<pid>.token` (Windows) ou
/// l'équivalent XDG data dir ailleurs.
fn default_token_file() -> PathBuf {
    let base = directories::BaseDirs::new()
        .map(|dirs| dirs.data_local_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    base.join("plstudio")
        .join("control")
        .join(format!("cli-{}.token", std::process::id()))
}

/// Écrit le token (une ligne hexadécimale) et restreint ses permissions
/// sur les systèmes qui savent le faire.
fn write_token_file(path: &std::path::Path, token: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(path, format!("{token}\n"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(path, permissions)?;
    }

    Ok(())
}

fn print_serve_usage() {
    println!("USAGE:");
    println!(
        "    plstudio-cli serve [--port n] [--workspace dir] [--trust trusted|restricted] [--token-file path]"
    );
}

/// Parse les arguments de `run` ; renvoie un message d'usage en cas d'erreur.
fn parse_run_args(args: &[String]) -> Result<RunSpec, String> {
    let mut command: Option<String> = None;
    let mut params: Option<Value> = None;
    let mut workspace = PathBuf::from(".");
    let mut trust = TrustLevel::Trusted;

    let mut index = 0;
    while index < args.len() {
        let (parameter, value) = match (args[index].as_str(), args.get(index + 1)) {
            (key @ ("--cmd" | "--param" | "--workspace" | "--trust"), Some(value)) => (key, value),
            (key @ ("--cmd" | "--param" | "--workspace" | "--trust"), None) => {
                return Err(format!("missing value after {key}"));
            }
            (other_flag, _) => return Err(format!("unknown `run` flag: {other_flag}")),
        };

        match parameter {
            "--cmd" => command = Some(value.clone()),
            "--param" => {
                params = Some(
                    serde_json::from_str(value)
                        .map_err(|error| format!("--param is not valid JSON: {error}"))?,
                );
            }
            "--workspace" => workspace = PathBuf::from(value),
            "--trust" => {
                trust = match value.as_str() {
                    "trusted" => TrustLevel::Trusted,
                    "restricted" => TrustLevel::Restricted,
                    other => {
                        return Err(format!(
                            "unknown trust level: {other} (expected trusted|restricted)"
                        ));
                    }
                };
            }
            _ => {
                return Err(format!("unhandled flag: {parameter}"));
            }
        }

        index += 2;
    }

    let command = command.ok_or_else(|| "missing --cmd <command-id>".to_string())?;

    Ok(RunSpec {
        command: CommandId::from(command.as_str()),
        // Les paramètres absents valent objet vide : convention Phase 1.
        params: params.unwrap_or_else(|| json!({})),
        workspace,
        trust,
    })
}

/// Sérialise une erreur plstudio en JSON structuré pour la machine.
fn error_payload(error: &PlsError) -> Value {
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

    let mut payload = json!({
        "code": error.code().as_str(),
        "message": message,
    });

    if let Some(remediation) = remediation {
        payload
            .as_object_mut()
            .expect("payload is a json object")
            .insert("remediation".to_string(), json!(remediation));
    }

    payload
}

/// JSON lisible, avec repli textuel si la sérialisation échoue (jamais vu).
fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|error| format!("{{\"serialization_error\":\"{error}\"}}"))
}

fn eprint_json(value: &Value) {
    eprintln!("{}", pretty(value));
}

fn print_run_usage() {
    println!("USAGE:");
    println!(
        "    plstudio-cli run --cmd <command-id> [--param '{{...}}'] [--workspace dir] [--trust trusted|restricted]"
    );
}

fn print_help() {
    println!("plstudio-cli {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("USAGE:");
    println!("    plstudio-cli <subcommand>");
    println!();
    println!("SUBCOMMANDS:");
    println!("    run         Execute an engine command");
    println!("    serve       Host the plstudio-control/1 channel on local TCP");
    println!("    commands    List registered commands");
    println!("    version     Print the version");
    println!("    help        Print this help");
    println!();
    println!("EXAMPLES:");
    println!("    plstudio-cli commands --json");
    print_run_usage();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_args_default_to_an_empty_object_and_local_workspace() {
        let spec = parse_run_args(&["--cmd".into(), "editor.list".into()]).expect("parses");
        assert_eq!(spec.command.as_str(), "editor.list");
        assert_eq!(spec.params, json!({}));
        assert_eq!(spec.workspace, PathBuf::from("."));
        assert!(spec.trust.is_trusted());
    }

    #[test]
    fn run_args_parse_all_flags() {
        let spec = parse_run_args(&[
            "--cmd".into(),
            "editor.open".into(),
            "--param".into(),
            r#"{"file": "lib/main.dart"}"#.into(),
            "--workspace".into(),
            "C:\\projects\\demo".into(),
            "--trust".into(),
            "restricted".into(),
        ])
        .expect("parses");

        assert_eq!(spec.command.as_str(), "editor.open");
        assert_eq!(
            spec.params.get("file").and_then(Value::as_str),
            Some("lib/main.dart")
        );
        assert_eq!(spec.workspace, PathBuf::from("C:\\projects\\demo"));
        assert!(!spec.trust.is_trusted());
    }

    #[test]
    fn run_args_reject_bad_usage() {
        // Commande manquante.
        assert!(parse_run_args(&["--param".into(), "{}".into()]).is_err());
        // JSON invalide.
        assert!(
            parse_run_args(&["--cmd".into(), "x".into(), "--param".into(), "{".into()]).is_err()
        );
        // Flag inconnu.
        assert!(parse_run_args(&["--cmd".into(), "x".into(), "--yolo".into()]).is_err());
        // Valeur manquante.
        assert!(parse_run_args(&["--cmd".into()]).is_err());
        // Niveau de confiance inconnu.
        assert!(
            parse_run_args(&["--cmd".into(), "x".into(), "--trust".into(), "gamma".into()])
                .is_err()
        );
    }

    #[test]
    fn serve_args_default_to_ephemeral_and_restricted() {
        let spec = parse_serve_args(&[]).expect("empty args parse");

        assert_eq!(spec.port, 0, "ephemeral port by default");
        assert_eq!(spec.workspace, PathBuf::from("."));
        assert!(
            !spec.trust.is_trusted(),
            "the channel defaults to read-only sessions"
        );
        assert!(
            spec.token_file.is_none(),
            "defaults to the platform token path"
        );
    }

    #[test]
    fn serve_args_parse_all_flags() {
        let spec = parse_serve_args(&[
            "--port".into(),
            "9320".into(),
            "--workspace".into(),
            "C:\\projects\\demo".into(),
            "--trust".into(),
            "trusted".into(),
            "--token-file".into(),
            "secret.token".into(),
        ])
        .expect("parses");

        assert_eq!(spec.port, 9320);
        assert_eq!(spec.workspace, PathBuf::from("C:\\projects\\demo"));
        assert!(spec.trust.is_trusted());
        assert_eq!(spec.token_file, Some(PathBuf::from("secret.token")));
    }

    #[test]
    fn serve_args_reject_bad_usage() {
        // Port non numérique.
        assert!(parse_serve_args(&["--port".into(), "seven".into()]).is_err());
        // Confiance inconnue.
        assert!(parse_serve_args(&["--trust".into(), "gamma".into()]).is_err());
        // Flag inconnu.
        assert!(parse_serve_args(&["--yolo".into(), "1".into()]).is_err());
        // Valeur manquante.
        assert!(parse_serve_args(&["--port".into()]).is_err());
    }

    #[test]
    fn errors_serialize_with_code_and_remediation() {
        let error = PlsError::app(ErrorCode::COMMAND, "unknown command `x`")
            .with_remediation("run `plstudio-cli commands`");
        let payload = error_payload(&error);

        assert_eq!(
            payload.get("code").and_then(Value::as_str),
            Some("PLS-E-1003")
        );
        assert_eq!(
            payload.get("message").and_then(Value::as_str),
            Some("unknown command `x`")
        );
        assert_eq!(
            payload.get("remediation").and_then(Value::as_str),
            Some("run `plstudio-cli commands`")
        );
    }
}
