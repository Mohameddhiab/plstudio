use std::{collections::HashMap, process::Command};

use serde_json::Value;

/// Couche de chaque crate. `pls-testkit` est hors couches (255) et exempté.
const LAYERS: &[(&str, u8)] = &[
    ("pls-base", 0),
    ("pls-text", 1),
    ("pls-sdk", 1),
    ("pls-avd", 2),
    ("pls-project", 2),
    ("pls-lsp", 3),
    ("pls-run", 3),
    ("pls-emulator", 3),
    ("pls-inspector", 3),
    ("pls-agent", 3),
    ("pls-engine", 4),
    ("pls-mcp", 5),
    ("pls-ui", 5),
    ("plstudio", 6),
    ("plstudio-cli", 6),
    ("xtask", 7),
    ("pls-testkit", 255),
];

const TESTKIT: &str = "pls-testkit";

pub fn check() -> Result<(), String> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .map_err(|error| format!("cannot run cargo metadata: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let metadata: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("cannot parse cargo metadata: {error}"))?;

    let packages = metadata
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| "cargo metadata has no packages array".to_owned())?;

    let layer_map: HashMap<&str, u8> = LAYERS.iter().copied().collect();
    let mut violations = Vec::new();
    let mut checked = 0usize;

    for package in packages {
        let Some(name) = package.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(&layer) = layer_map.get(name) else {
            continue;
        };

        checked += 1;

        if name == TESTKIT {
            continue;
        }

        let dependencies = package
            .get("dependencies")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        for dependency in dependencies {
            // Les dépendances de dev/build ont un `kind` non nul.
            if dependency.get("kind").and_then(Value::as_str).is_some() {
                continue;
            }

            let Some(dependency_name) = dependency.get("name").and_then(Value::as_str) else {
                continue;
            };
            let Some(&dependency_layer) = layer_map.get(dependency_name) else {
                continue;
            };

            if dependency_name == TESTKIT || layer == 255 {
                continue;
            }

            if dependency_layer >= layer {
                violations.push(format!(
                    "{name} (layer {layer}) must not depend on {dependency_name} (layer {dependency_layer})"
                ));
            }
        }
    }

    if violations.is_empty() {
        println!("layers: OK ({checked} workspace crates)");
        Ok(())
    } else {
        let mut message = String::from("layer violations:\n");
        for violation in &violations {
            message.push_str("  - ");
            message.push_str(violation);
            message.push('\n');
        }
        Err(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layer_is_below_the_testkit_sentinel() {
        for (name, layer) in LAYERS {
            if *name == TESTKIT {
                assert_eq!(*layer, 255);
            } else {
                assert!(*layer < 255);
            }
        }
    }
}
