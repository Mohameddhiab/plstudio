use std::fmt;

/// Code d'erreur stable exposé aux utilisateurs, aux agents et aux clients MCP.
///
/// Les codes sont documentés et ne doivent pas changer sans migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ErrorCode(&'static str);

impl ErrorCode {
    pub const IO: Self = Self("PLS-E-1001");
    pub const CONFIG: Self = Self("PLS-E-1002");
    pub const COMMAND: Self = Self("PLS-E-1003");
    pub const INVALID_PARAMS: Self = Self("PLS-E-1004");
    pub const NOT_IMPLEMENTED: Self = Self("PLS-E-1005");
    pub const PERMISSION: Self = Self("PLS-E-1006");
    pub const CANCELLED: Self = Self("PLS-E-1007");
    pub const LAYER: Self = Self("PLS-E-2001");

    pub const fn new(code: &'static str) -> Self {
        Self(code)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

/// Erreur applicative de plstudio.
#[derive(Debug, thiserror::Error)]
pub enum PlsError {
    #[error("[{code}] {message}")]
    App {
        code: ErrorCode,
        message: String,
        remediation: Option<String>,
    },
    #[error("[{code}] {message}")]
    Config {
        code: ErrorCode,
        message: String,
        remediation: Option<String>,
    },
    #[error("[{code}] I/O error: {source}")]
    Io {
        code: ErrorCode,
        #[source]
        source: std::io::Error,
    },
}

impl PlsError {
    pub fn app(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::App {
            code,
            message: message.into(),
            remediation: None,
        }
    }

    pub fn config(message: impl Into<String>) -> Self {
        Self::Config {
            code: ErrorCode::CONFIG,
            message: message.into(),
            remediation: None,
        }
    }

    pub fn not_implemented(what: &'static str) -> Self {
        Self::app(
            ErrorCode::NOT_IMPLEMENTED,
            format!("not implemented: {what}"),
        )
    }

    pub fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        match &mut self {
            Self::App {
                remediation: slot, ..
            }
            | Self::Config {
                remediation: slot, ..
            } => *slot = Some(remediation.into()),
            Self::Io { .. } => {}
        }
        self
    }

    pub fn code(&self) -> ErrorCode {
        match self {
            Self::App { code, .. } | Self::Config { code, .. } | Self::Io { code, .. } => *code,
        }
    }
}

impl From<std::io::Error> for PlsError {
    fn from(source: std::io::Error) -> Self {
        Self::Io {
            code: ErrorCode::IO,
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, PlsError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_has_stable_code_and_remediation() {
        let error = PlsError::app(ErrorCode::COMMAND, "unknown command")
            .with_remediation("run `plstudio-cli commands`");
        assert_eq!(error.code(), ErrorCode::COMMAND);
        assert!(error.to_string().contains("PLS-E-1003"));
        assert!(error.to_string().contains("unknown command"));
    }
}
