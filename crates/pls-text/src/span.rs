//! Tranches de caractères et validation des éditions.

use serde::{Deserialize, Serialize};

use crate::{ErrorCode, PlsError, Result};

/// Tranche de texte par indices de caractères (unité LSP-agnostique).
///
/// `end` est exclusif. Une tranche avec `start == end` insère sans supprimer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharSpan {
    pub start: usize,
    pub end: usize,
}

impl CharSpan {
    /// Tranche d'insertion pure : `[index, index)` (aucun caractère supprimé).
    pub fn empty_at(index: usize) -> Self {
        Self {
            start: index,
            end: index,
        }
    }

    /// Valide la tranche contre la longueur du buffer en caractères.
    pub fn validate(&self, len_chars: usize) -> Result<()> {
        if self.start > self.end {
            return Err(PlsError::app(
                ErrorCode::INVALID_PARAMS,
                format!(
                    "char span {}..{} is reversed; start must not exceed end",
                    self.start, self.end
                ),
            )
            .with_remediation("swap start and end, or derive the span from find()"));
        }

        if self.end > len_chars {
            return Err(PlsError::app(
                ErrorCode::INVALID_PARAMS,
                format!(
                    "char span {}..{} exceeds buffer length ({len_chars} chars)",
                    self.start, self.end
                ),
            )
            .with_remediation("recompute the span, or open the buffer again"));
        }

        Ok(())
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_limits_and_order() {
        assert!(CharSpan { start: 2, end: 4 }.validate(4).is_ok());
        assert!(CharSpan::empty_at(4).validate(4).is_ok());
        assert!(CharSpan { start: 3, end: 2 }.validate(4).is_err());
        assert!(CharSpan { start: 0, end: 5 }.validate(4).is_err());
    }
}
