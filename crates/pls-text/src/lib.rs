//! Buffers de texte et édition de base.
//!
//! Socle d'édition headless commun aux trois candidats UI (SPK-01) et aux
//! commandes `editor.*` de la Phase 1 : éditions triviales avec indices de
//! caractères, positions ligne:colonne, recherche, undo/redo groupé et
//! transactions annulables.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use pls_base::{ErrorCode, PlsError, Result};
use ropey::Rope;

pub mod caret;
pub mod history;
pub mod position;
pub mod search;
pub mod span;

pub use caret::Caret;
pub use history::History;
pub use position::{Position, line_len_without_eol};
pub use search::{find_all, find_next};
pub use span::CharSpan;

/// Buffer de texte en mémoire, adossé à un fichier optionnel.
#[derive(Debug, Clone)]
pub struct TextBuffer {
    rope: Rope,
    path: Option<PathBuf>,
    dirty: bool,
    history: History,
    /// Éditions accumulées dans une transaction ouverte (voir [`Self::transaction`]).
    pending_group: Option<Vec<Caret>>,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self::from_text("")
    }

    pub fn from_text(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            path: None,
            dirty: false,
            history: History::new(),
            pending_group: None,
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        Ok(Self {
            rope: Rope::from_str(&text),
            path: Some(path.to_path_buf()),
            dirty: false,
            history: History::new(),
            pending_group: None,
        })
    }

    pub fn save(&mut self) -> Result<()> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| PlsError::config("buffer has no path; use save_as"))?;
        self.save_as(path)
    }

    pub fn save_as(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        let path = path.into();
        std::fs::write(&path, self.text())?;
        self.path = Some(path);
        self.dirty = false;
        Ok(())
    }

    /// Insère du texte à un indice de caractère.
    pub fn insert(&mut self, char_index: usize, text: &str) -> Result<()> {
        self.edit(CharSpan::empty_at(char_index), text)?;
        Ok(())
    }

    /// Supprime la tranche donnée.
    pub fn delete(&mut self, span: CharSpan) -> Result<Caret> {
        self.edit(span, "")
    }

    /// Remplace la tranche donnée par un texte, en enregistrant l'undo.
    ///
    /// Une édition sans effet (texte identique) ne touche ni le buffer ni
    /// l'historique.
    pub fn edit(&mut self, span: CharSpan, text: &str) -> Result<Caret> {
        span.validate(self.rope.len_chars())?;

        let removed = self.rope.slice(span.start..span.end).to_string();
        let caret = Caret::change(span.start, &removed, text);

        if !caret.is_noop() {
            caret.apply(&mut self.rope);
            self.dirty = true;
        }

        self.record(caret.clone());
        Ok(caret)
    }

    /// Remplace toutes les occurrences de `needle`, groupées en un seul undo.
    ///
    /// La progression n'utilise jamais chevauchement ni boucle : la recherche
    /// repart après le texte inséré, sur le buffer mis à jour.
    pub fn replace_all(&mut self, needle: &str, replacement: &str) -> Result<usize> {
        if needle.is_empty() {
            return Err(
                PlsError::app(ErrorCode::INVALID_PARAMS, "the search needle is empty")
                    .with_remediation("provide a non-empty needle"),
            );
        }

        let mut count = 0_usize;

        self.transaction(|buffer| {
            let mut from = 0_usize;

            while let Some(span) = buffer.find(needle, from)? {
                buffer.edit(span, replacement)?;
                count += 1;
                from = span.start + replacement.chars().count();
            }

            Ok(())
        })?;

        Ok(count)
    }

    /// Annule la dernière transaction ; renvoie vrai si quelque chose a été annulé.
    pub fn undo(&mut self) -> bool {
        self.history.undo(&mut self.rope)
    }

    /// Rejoue la dernière transaction annulée.
    pub fn redo(&mut self) -> bool {
        self.history.redo(&mut self.rope)
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Exécute un corps d'éditions comme une transaction annulable.
    ///
    /// Les éditions du corps forment un seul pas d'undo ; un imbriqué fusionne
    /// dans le groupe extérieur. Si le corps échoue, toutes ses éditions sont
    /// annulées et l'erreur remonte.
    pub fn transaction<T>(&mut self, body: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let outer = self.pending_group.replace(Vec::new());
        let dirty_before = self.dirty;
        let outcome = body(self);
        let pending = self.pending_group.take();
        self.pending_group = outer;

        match outcome {
            Ok(value) => {
                if let Some(pending) = pending {
                    match self.pending_group.as_mut() {
                        // Transaction imbriquée : les éditions remontent au groupe extérieur.
                        Some(outer) => outer.extend(pending),
                        None => {
                            self.history.record_group(pending);
                        }
                    }
                }
                Ok(value)
            }
            Err(error) => {
                if let Some(pending) = pending {
                    for caret in pending.iter().rev() {
                        caret.apply_inverse(&mut self.rope);
                    }
                }
                // Le contenu est restauré : le buffer n'a pas été modifié.
                self.dirty = dirty_before;
                Err(error)
            }
        }
    }

    fn record(&mut self, caret: Caret) {
        if let Some(pending) = self.pending_group.as_mut() {
            pending.push(caret);
        } else {
            self.history.record_group(vec![caret]);
        }
    }

    /// Trouve la première occurrence de `needle` à partir d'un indice.
    pub fn find(&self, needle: &str, from_char: usize) -> Result<Option<CharSpan>> {
        find_next(&self.rope, needle, from_char)
    }

    /// Trouve toutes les occurrences de `needle`, sans chevauchement.
    pub fn find_all(&self, needle: &str) -> Result<Vec<CharSpan>> {
        find_all(&self.rope, needle)
    }

    /// Convertit une position ligne:colonne en indice de caractère global.
    pub fn position_to_char(&self, position: Position) -> Result<usize> {
        position.to_char(&self.rope)
    }

    /// Convertit un indice de caractère global en position ligne:colonne.
    pub fn char_to_position(&self, char_index: usize) -> Result<Position> {
        Position::from_char(&self.rope, char_index)
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn line(&self, line_index: usize) -> Option<String> {
        if line_index >= self.rope.len_lines() {
            return None;
        }

        Some(self.rope.line(line_index).to_string())
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_text_at_a_char_index() {
        let mut buffer = TextBuffer::from_text("fn main() {}\n");
        buffer.insert(3, "hello ").expect("insert");
        assert_eq!(buffer.text(), "fn hello main() {}\n");
        assert!(buffer.is_dirty());
    }

    #[test]
    fn rejects_out_of_bounds_insert() {
        let mut buffer = TextBuffer::from_text("");
        assert!(buffer.insert(1, "x").is_err());
    }

    #[test]
    fn saves_and_reopens_a_file() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("main.dart");

        let mut buffer = TextBuffer::from_text("void main() {}\n");
        buffer.save_as(&path).expect("save");
        assert!(!buffer.is_dirty());

        let reopened = TextBuffer::open(&path).expect("open");
        assert_eq!(reopened.text(), "void main() {}\n");
        assert_eq!(reopened.path(), Some(path.as_path()));
    }

    #[test]
    fn deletes_a_span_and_undoes_it() {
        let mut buffer = TextBuffer::from_text("hello world\n");
        let caret = buffer
            .delete(CharSpan { start: 5, end: 11 })
            .expect("delete");
        assert_eq!(caret.removed, " world");
        assert_eq!(buffer.text(), "hello\n");

        assert!(buffer.undo());
        assert_eq!(buffer.text(), "hello world\n");
        assert!(buffer.redo());
        assert_eq!(buffer.text(), "hello\n");
    }

    #[test]
    fn replaces_a_span() {
        let mut buffer = TextBuffer::from_text("fn main() {}\n");
        buffer
            .edit(CharSpan { start: 3, end: 7 }, "test")
            .expect("replace");
        assert_eq!(buffer.text(), "fn test() {}\n");
        assert!(buffer.can_undo());
        assert!(!buffer.can_redo());
    }

    #[test]
    fn identical_edit_changes_nothing() {
        let mut buffer = TextBuffer::from_text("hello\n");
        let inside = buffer
            .edit(CharSpan { start: 0, end: 5 }, "hello")
            .expect("edit");

        assert!(caret_is_noop(&inside));
        assert!(!buffer.is_dirty());
        assert!(!buffer.can_undo());
        assert_eq!(buffer.text(), "hello\n");
    }

    fn caret_is_noop(caret: &Caret) -> bool {
        caret.removed == caret.inserted
    }

    #[test]
    fn rejects_reversed_or_oversized_spans() {
        let mut buffer = TextBuffer::from_text("abc");
        assert!(buffer.edit(CharSpan { start: 2, end: 1 }, "x").is_err());
        assert!(buffer.edit(CharSpan { start: 0, end: 4 }, "x").is_err());
        assert!(buffer.delete(CharSpan { start: 0, end: 0 }).is_ok());
        assert_eq!(buffer.text(), "abc");
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn replace_all_is_one_undo_step() {
        let mut buffer = TextBuffer::from_text("ab ab ab\n");
        let count = buffer.replace_all("ab", "cd").expect("replace_all");
        assert_eq!(count, 3);
        assert_eq!(buffer.text(), "cd cd cd\n");

        assert!(buffer.undo());
        assert_eq!(buffer.text(), "ab ab ab\n");
        assert!(buffer.redo());
        assert_eq!(buffer.text(), "cd cd cd\n");
    }

    #[test]
    fn replace_all_with_empty_replacement_terminates() {
        let mut buffer = TextBuffer::from_text("aaa\n");
        let count = buffer.replace_all("aa", "").expect("replace_all");
        assert_eq!(count, 1);
        assert_eq!(buffer.text(), "a\n");
    }

    #[test]
    fn replace_all_grows_the_text_safely() {
        let mut buffer = TextBuffer::from_text("abab");
        let count = buffer.replace_all("a", "ab").expect("replace_all");
        assert_eq!(count, 2);
        assert_eq!(buffer.text(), "abbabb");

        assert!(buffer.undo());
        assert_eq!(buffer.text(), "abab");
    }

    #[test]
    fn replace_all_rejects_an_empty_needle() {
        let mut buffer = TextBuffer::from_text("abc");
        assert!(buffer.replace_all("", "x").is_err());
        assert_eq!(buffer.text(), "abc");
    }

    #[test]
    fn transaction_success_is_single_undo_group() {
        let mut buffer = TextBuffer::from_text("abc\n");

        buffer
            .transaction(|buffer| {
                buffer.edit(CharSpan { start: 0, end: 1 }, "X")?;
                buffer.edit(CharSpan::empty_at(3), "d")?;
                Ok(())
            })
            .expect("transaction");

        assert_eq!(buffer.text(), "Xbcd\n");
        assert!(buffer.undo());
        assert_eq!(buffer.text(), "abc\n");
        assert!(!buffer.can_undo());
    }

    #[test]
    fn transaction_failure_rolls_back() {
        let mut buffer = TextBuffer::from_text("abc\n");

        let outcome: Result<()> = buffer.transaction(|buffer| {
            buffer.edit(CharSpan { start: 0, end: 1 }, "X")?;
            Err(PlsError::app(ErrorCode::COMMAND, "boom"))
        });

        assert!(outcome.is_err());
        assert_eq!(buffer.text(), "abc\n");
        assert!(!buffer.can_undo());
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn nested_transactions_merge_into_one_group() {
        let mut buffer = TextBuffer::from_text("abc\n");

        buffer
            .transaction(|buffer| {
                buffer.edit(CharSpan { start: 0, end: 1 }, "X")?;
                buffer.transaction(|buffer| buffer.edit(CharSpan { start: 1, end: 2 }, "Y"))?;
                Ok(())
            })
            .expect("transaction");

        assert_eq!(buffer.text(), "XYc\n");
        assert!(buffer.undo());
        assert_eq!(buffer.text(), "abc\n");
    }

    #[test]
    fn sequential_edits_undo_step_by_step() {
        let mut buffer = TextBuffer::from_text("");
        buffer.insert(0, "a").expect("insert");
        buffer.insert(1, "b").expect("insert");
        buffer.insert(2, "c").expect("insert");
        assert_eq!(buffer.text(), "abc");

        assert!(buffer.undo());
        assert_eq!(buffer.text(), "ab");
        assert!(buffer.undo());
        assert_eq!(buffer.text(), "a");
        assert!(buffer.undo());
        assert_eq!(buffer.text(), "");
        assert!(!buffer.undo());
        assert!(buffer.redo());
        assert_eq!(buffer.text(), "a");
        assert!(buffer.redo());
        assert_eq!(buffer.text(), "ab");
        assert!(buffer.redo());
        assert_eq!(buffer.text(), "abc");
    }

    #[test]
    fn positions_round_trip_with_crlf() {
        let rope = Rope::from_str("héllo\r\nworld\r\n");
        assert_eq!(line_len_without_eol(&rope, 0), 5);

        let buffer = TextBuffer::from_text("héllo\r\nworld\r\n");
        let index = buffer
            .position_to_char(Position { line: 1, col: 2 })
            .expect("position");
        assert_eq!(
            buffer.char_to_position(index).expect("position"),
            Position { line: 1, col: 2 }
        );

        // L'indice sur le terminateur se borne à la longueur utile.
        let eol_index = buffer
            .position_to_char(Position { line: 0, col: 5 })
            .expect("position");
        assert_eq!(
            buffer.char_to_position(eol_index).expect("position"),
            Position { line: 0, col: 5 }
        );
    }

    #[test]
    fn end_of_document_positions_follow_the_trailing_newline() {
        let with_newline = TextBuffer::from_text("ab\n");
        let end = with_newline
            .char_to_position(with_newline.len_chars())
            .expect("position");
        assert_eq!(end, Position { line: 2, col: 0 });
        assert_eq!(
            with_newline
                .position_to_char(Position { line: 2, col: 0 })
                .expect("position"),
            with_newline.len_chars()
        );

        let without_newline = TextBuffer::from_text("ab");
        let end = without_newline.char_to_position(2).expect("position");
        assert_eq!(end, Position { line: 0, col: 2 });

        let empty = TextBuffer::from_text("");
        assert_eq!(
            empty.char_to_position(0).expect("position"),
            Position { line: 0, col: 0 }
        );
        // Sans \n final, la bonne position de fin est {0, 0}, pas {1, 0}.
        assert!(
            empty
                .position_to_char(Position { line: 1, col: 0 })
                .is_err()
        );
    }

    #[test]
    fn positions_outside_the_document_are_rejected() {
        let buffer = TextBuffer::from_text("ab\n");
        assert!(
            buffer
                .position_to_char(Position { line: 3, col: 0 })
                .is_err()
        );
        assert!(
            buffer
                .position_to_char(Position { line: 0, col: 3 })
                .is_err()
        );
        assert!(buffer.char_to_position(4).is_err());
    }
}
