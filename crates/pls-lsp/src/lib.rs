//! Client LSP générique.
//!
//! Le transport et le cycle de vie arrivent en Phase 2.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspServerSpec {
    pub language: String,
    pub command: String,
    pub args: Vec<String>,
}

impl LspServerSpec {
    pub fn new(language: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            language: language.into(),
            command: command.into(),
            args: Vec::new(),
        }
    }

    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub message: String,
    pub severity: DiagnosticSeverity,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_dart_server_spec() {
        let spec = LspServerSpec::new("dart", "dart").with_args(["language-server"]);
        assert_eq!(spec.args, ["language-server"]);
    }
}
