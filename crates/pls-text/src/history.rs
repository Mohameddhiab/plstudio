//! Pile undo/redo groupée par transactions.
//!
//! Un groupe = un pas d'undo. Un groupe peut contenir plusieurs éditions
//! (remplacement global, transaction de l'agent plus tard). La pile est
//! bornée : les groupes les plus anciens sont abandonnés silencieusement,
//! comme dans tout éditeur de code.
//!
//! Invariant : les indices de chaque edition sont valides au moment où elle
//! est rejouée. C'est garanti par la troncature du redo à chaque nouvelle
//! édition et par l'application LIFO/FIFO stricte ; une modification externe
//! du fichier (hors moteur) invalide la pile, ce que l'engine détectera via
//! watcher + hash (Phase 2).

use std::collections::VecDeque;

use ropey::Rope;

use crate::caret::Caret;

const MAX_GROUPS: usize = 1024;

#[derive(Debug, Clone, Default)]
pub struct History {
    groups: VecDeque<Vec<Caret>>,
    /// Nombre de groupes appliqués (== indice du prochain redo). `undo_body`
    /// rejoue l'inverse de `groups[cursor - 1]`.
    cursor: usize,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enregistre un groupe d'éditions et renvoie vrai s'il a modifié son contenu.
    pub fn record_group(&mut self, carets: Vec<Caret>) -> bool {
        let meaningful: Vec<Caret> = carets
            .into_iter()
            .filter(|caret| !caret.is_noop())
            .collect();
        if meaningful.is_empty() {
            return false;
        }

        // Toute nouvelle édition invalide le redo.
        self.groups.truncate(self.cursor);
        self.groups.push_back(meaningful);
        self.cursor += 1;

        while self.groups.len() > MAX_GROUPS {
            self.groups.pop_front();
            self.cursor -= 1;
        }

        true
    }

    pub fn undo(&mut self, rope: &mut Rope) -> bool {
        if self.cursor == 0 {
            return false;
        }

        let caret_group = self.cursor - 1;
        for caret in self.groups[caret_group].iter().rev() {
            caret.apply_inverse(rope);
        }
        self.cursor -= 1;

        true
    }

    pub fn redo(&mut self, rope: &mut Rope) -> bool {
        if self.cursor >= self.groups.len() {
            return false;
        }

        let caret_group = self.cursor;
        for caret in self.groups[caret_group].iter() {
            caret.apply(rope);
        }
        self.cursor += 1;

        true
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor < self.groups.len()
    }

    pub fn group_count(&self) -> usize {
        self.groups.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rope(text: &str) -> Rope {
        Rope::from_str(text)
    }

    /// Applique le caret sur la rope, comme le fait `TextBuffer::edit`.
    fn apply_forward(rope: &mut Rope, caret: Caret) {
        caret.apply(rope);
    }

    #[test]
    fn single_undo_redo_roundtrip() {
        let mut rope = rope("abc");
        let mut history = History::new();

        let caret = Caret::change(0, "", "hello ");
        apply_forward(&mut rope, caret.clone());
        history.record_group(vec![caret]);
        assert!(history.undo(&mut rope));
        assert_eq!(rope.to_string(), "abc");
        assert!(history.redo(&mut rope));
        assert_eq!(rope.to_string(), "hello abc");
    }

    #[test]
    fn grouped_edits_undo_in_reverse_order() {
        let mut rope = rope("xx y y");
        let mut history = History::new();

        let group = vec![Caret::change(0, "xx", "zz"), Caret::change(5, "y", "w")];
        for caret in group.clone() {
            apply_forward(&mut rope, caret);
        }
        history.record_group(group);

        assert!(history.undo(&mut rope));
        assert_eq!(rope.to_string(), "xx y y");
        assert!(history.redo(&mut rope));
        assert_eq!(rope.to_string(), "zz y w");
    }

    #[test]
    fn new_edit_truncates_redo() {
        let mut rope = rope("");
        let mut history = History::new();

        let first = Caret::change(0, "", "a");
        apply_forward(&mut rope, first.clone());
        history.record_group(vec![first]);
        let second = Caret::change(1, "", "b");
        apply_forward(&mut rope, second.clone());
        history.record_group(vec![second]);
        assert!(history.undo(&mut rope));
        assert_eq!(rope.to_string(), "a");

        let third = Caret::change(1, "", "c");
        apply_forward(&mut rope, third.clone());
        history.record_group(vec![third]);
        assert!(history.can_undo());
        assert!(!history.can_redo());
        assert!(history.undo(&mut rope));
        assert_eq!(rope.to_string(), "a");
    }

    #[test]
    fn noop_edits_are_not_recorded() {
        let rope = rope("abc");
        let mut history = History::new();

        assert!(!history.record_group(vec![Caret::change(1, "b", "b")]));
        assert!(!history.can_undo());
        assert_eq!(rope.to_string(), "abc");
    }

    #[test]
    fn history_is_capped() {
        let mut rope = rope("");
        let mut history = History::new();

        for index in 0..(MAX_GROUPS + 50) {
            let text = index.to_string();
            history.record_group(vec![Caret::change(rope.len_chars(), "", text.as_str())]);
            rope.insert(rope.len_chars(), text.as_str());
        }

        assert!(history.group_count() <= MAX_GROUPS);

        // Tout le passé n'est plus annulable.
        let mut undos = 0;
        while history.undo(&mut rope) {
            undos += 1;
        }
        assert!(undos <= MAX_GROUPS);
        assert!(rope.len_chars() > 0);
    }
}
