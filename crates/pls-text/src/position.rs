//! Positions ligne:colonne et conversions vers des indices de caractères.
//!
//! Spécification Phase 1 : `col` est compté en **caractères**, hors terminateur
//! de ligne. La correspondance UTF-16 (contrat LSP exact) sera ajoutée dans
//! `pls-lsp` ; ces fonctions restent le socle commun des trois candidats UI.
//!
//! Convention de fin de document : un carret après le dernier caractère vaut
//! `{line: len_lines, col: 0}` quand le buffer se termine par `\n`, sinon la
//! fin de la dernière ligne. Elle sert aux éditions en bout de fichier,
//! jamais à la lecture de texte.

use serde::{Deserialize, Serialize};

use ropey::Rope;

use crate::{ErrorCode, PlsError, Result};

/// Position d'un caractère dans le buffer, à partir de zéro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

impl Position {
    pub const START: Self = Self { line: 0, col: 0 };

    /// Conversion position → indice de caractère global.
    pub fn to_char(self, rope: &Rope) -> Result<usize> {
        let len_lines = rope.len_lines();

        if self.line >= len_lines {
            if self.line == len_lines && self.col == 0 && ends_with_newline(rope) {
                return Ok(rope.len_chars());
            }

            return Err(PlsError::app(
                ErrorCode::INVALID_PARAMS,
                format!(
                    "line {} is outside the document ({len_lines} lines)",
                    self.line
                ),
            )
            .with_remediation(
                "recompute the line from the previous view, or start from Position::START",
            ));
        }

        let line_start = rope
            .try_line_to_char(self.line)
            .map_err(|_| out_of_range(self.line, len_lines))?;
        let line_len = line_len_without_eol(rope, self.line);

        if self.col > line_len {
            return Err(PlsError::app(
                ErrorCode::INVALID_PARAMS,
                format!(
                    "column {} exceeds line {} length ({line_len} chars)",
                    self.col, self.line
                ),
            )
            .with_remediation("recompute the column, or clamp it to the line length"));
        }

        Ok(line_start + self.col)
    }

    /// Conversion indice de caractère global → position.
    ///
    /// L'indice égal à `len_chars` (fin du carret) est borné au lieu d'être
    /// rejeté : ce point est une position d'édition valide.
    pub fn from_char(rope: &Rope, char_index: usize) -> Result<Position> {
        if char_index > rope.len_chars() {
            return Err(PlsError::app(
                ErrorCode::INVALID_PARAMS,
                format!(
                    "char index {char_index} exceeds buffer length ({} chars)",
                    rope.len_chars()
                ),
            ));
        }

        if char_index == rope.len_chars() {
            return Ok(end_of_document(rope));
        }

        let len_lines = rope.len_lines();
        let line = rope
            .char_to_line(char_index)
            .min(len_lines.saturating_sub(1));
        let line_start = rope.line_to_char(line);
        // L'indice peut pointer sur le terminateur : la colonne se borne à la
        // longueur utile de la ligne.
        let col = (char_index - line_start).min(line_len_without_eol(rope, line));

        Ok(Position { line, col })
    }
}

/// Longueur utile d'une ligne, terminateur exclu (`\n` ou `\r\n`).
pub fn line_len_without_eol(rope: &Rope, line: usize) -> usize {
    let start = rope.line_to_char(line);
    let total = rope.line(line).len_chars();

    if total == 0 {
        return 0;
    }

    // Le terminateur, s'il existe, est la fin de la tranche de ligne.
    let last = rope.char(start + total - 1);
    if last == '\n' {
        if total >= 2 && rope.char(start + total - 2) == '\r' {
            total - 2
        } else {
            total - 1
        }
    } else if last == '\r' {
        total - 1
    } else {
        total
    }
}

fn end_of_document(rope: &Rope) -> Position {
    if rope.len_chars() == 0 {
        return Position { line: 0, col: 0 };
    }

    let len_lines = rope.len_lines();
    if ends_with_newline(rope) {
        return Position {
            line: len_lines,
            col: 0,
        };
    }

    let last_line = len_lines - 1;
    Position {
        line: last_line,
        col: line_len_without_eol(rope, last_line),
    }
}

fn ends_with_newline(rope: &Rope) -> bool {
    rope.len_chars() > 0 && rope.char(rope.len_chars() - 1) == '\n'
}

fn out_of_range(line: usize, len_lines: usize) -> PlsError {
    PlsError::app(
        ErrorCode::INVALID_PARAMS,
        format!("line {line} is outside the document ({len_lines} lines)"),
    )
    .with_remediation("recompute the line, or clamp it to the line count")
}
