//! Enregistrement d'une édition atomique, avec son inverse.
//!
//! Une même structure pilote l'application (redo), l'inversion (undo) et la
//! journalisation des mutations. Les indices sont des indices de caractères
//! valables **au moment de l'édition** : l'application des inverses se fait
//! en LIFO strict, et toute édition suivante invalide le redo (troncature).

use ropey::Rope;
use serde::{Deserialize, Serialize};

/// Une édition appliquée : à `start`, `removed` a été remplacé par `inserted`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caret {
    pub start: usize,
    pub removed: String,
    pub inserted: String,
}

impl Caret {
    pub fn change(start: usize, removed: &str, inserted: &str) -> Self {
        Self {
            start,
            removed: removed.to_string(),
            inserted: inserted.to_string(),
        }
    }

    /// Modification purement identitaire : rien à enregistrer.
    pub fn is_noop(&self) -> bool {
        self.removed == self.inserted
    }

    /// Applique l'édition (sens redo) sur la rope.
    pub(crate) fn apply(&self, rope: &mut Rope) {
        apply_change(rope, self.start, &self.removed, &self.inserted);
    }

    /// Applique l'inverse (sens undo) sur la rope.
    pub(crate) fn apply_inverse(&self, rope: &mut Rope) {
        apply_change(rope, self.start, &self.inserted, &self.removed);
    }
}

fn apply_change(rope: &mut Rope, start: usize, removed: &str, inserted: &str) {
    if !removed.is_empty() {
        let removed_chars = removed.chars().count();
        rope.remove(start..start + removed_chars);
    }

    if !inserted.is_empty() {
        rope.insert(start, inserted);
    }
}
