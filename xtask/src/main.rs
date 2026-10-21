//! Tâches de développement de plstudio.

#![forbid(unsafe_code)]

mod layers;

use std::{
    env,
    process::{Command, ExitCode},
};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("help");

    match command {
        "ci" => run_ci(),
        "layers" => run_layers(),
        "fmt" => run_or_fail(&["fmt", "--all"]),
        "check" => run_or_fail(&["check", "--workspace", "--all-targets"]),
        "clean" => run_or_fail(&["clean"]),
        "help" | "--help" | "-h" => {
            print_help();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown xtask command: {other}");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn run_ci() -> ExitCode {
    let commands: &[&[&str]] = &[
        &["fmt", "--all", "--", "--check"],
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        &["test", "--workspace"],
        &["run", "-p", "xtask", "--", "layers"],
    ];

    for command in commands {
        if !run(command) {
            return ExitCode::FAILURE;
        }
    }

    ExitCode::SUCCESS
}

fn run_layers() -> ExitCode {
    match layers::check() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run_or_fail(args: &[&str]) -> ExitCode {
    if run(args) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run(args: &[&str]) -> bool {
    eprintln!("+ cargo {}", args.join(" "));
    match Command::new("cargo").args(args).status() {
        Ok(status) => status.success(),
        Err(error) => {
            eprintln!("failed to run cargo {}: {error}", args.join(" "));
            false
        }
    }
}

fn print_help() {
    println!("plstudio xtask");
    println!();
    println!("USAGE:");
    println!("    cargo xtask <command>");
    println!();
    println!("COMMANDS:");
    println!("    ci        fmt + clippy + tests + layers");
    println!("    layers    Verify crate layer dependencies");
    println!("    fmt       Format the workspace");
    println!("    check     cargo check --workspace --all-targets");
    println!("    clean     cargo clean");
}
