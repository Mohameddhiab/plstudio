//! CLI headless de plstudio.
//!
//! Toute action passe par le moteur : `run --cmd <id> --param '{...}'`
//! exécute une commande et renvoie son résultat JSON. Le protocole de
//! contrôle longue durée et MCP arrivent dans les phases suivantes.

#![forbid(unsafe_code)]

use std::{env, path::PathBuf, process::ExitCode};

use pls_base::{CommandCtx, CommandId, ErrorCode, PlsError, TrustLevel};
use pls_engine::Engine;
use serde_json::{Value, json};

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
