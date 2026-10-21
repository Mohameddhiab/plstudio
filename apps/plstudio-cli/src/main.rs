//! CLI headless de plstudio.
//!
//! En Phase 0, elle expose le registre de commandes et la version. Le
//! protocole de contrôle et MCP arrivent dans les phases suivantes.

#![forbid(unsafe_code)]

use std::{env, process::ExitCode};

use pls_engine::Engine;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("--version" | "version") => {
            println!("plstudio-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("commands") => {
            let engine = Engine::new();
            let commands = engine.commands();

            if commands.is_empty() {
                println!("no commands registered (Phase 0)");
            } else {
                for meta in commands {
                    println!("{}\t{:?}\t{}", meta.id, meta.danger, meta.title);
                }
            }

            ExitCode::SUCCESS
        }
        Some("--help" | "-h" | "help") | None => {
            print_help();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown command: {other}");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn print_help() {
    println!("plstudio-cli {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("USAGE:");
    println!("    plstudio-cli <command>");
    println!();
    println!("COMMANDS:");
    println!("    commands    List registered commands");
    println!("    version     Print the version");
    println!("    help        Print this help");
}
