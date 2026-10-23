//! État d'édition partagé du moteur.
//!
//! Le store vit dans `pls-engine` et survit aux commandes : les buffers
//! ouverts restent en mémoire avec leur historique undo/redo. Un seul verrou
//! protège la table ; les opérations sont courtes, les résultats sont
//! copiés et jamais un guard ne s'échappe.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use pls_base::{BufferId, ErrorCode, PlsError, Result};
use serde::Serialize;

use pls_text::{Caret, CharSpan, Position, TextBuffer};

/// Buffer résumé pour la CLI, l'UI et MCP.
#[derive(Debug, Clone, Serialize)]
pub struct BufferSummary {
    pub id: BufferId,
    pub file: Option<PathBuf>,
    pub len_chars: usize,
    pub len_lines: usize,
    pub dirty: bool,
}

/// Occurrence trouvée : indices de caractères et position ligne:colonne.
#[derive(Debug, Clone, Serialize)]
pub struct MatchSpan {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

/// État de l'éditeur : buffers ouverts et identifiants.
#[derive(Default)]
pub struct EditorStore {
    next_id: AtomicU64,
    buffers: Mutex<BTreeMap<BufferId, TextBuffer>>,
}

impl EditorStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ouvre (ou réutilise) un buffer pour un fichier disque.
    ///
    /// Un fichier déjà ouvert n'est jamais écrasé : le buffer en mémoire fait
    /// foi. La réconciliation avec les modifications externes (watcher + hash)
    /// arrive en Phase 2.
    pub fn open(&self, file: &Path) -> Result<BufferSummary> {
        let file = file.to_path_buf();
        if !file.is_file() {
            return Err(PlsError::app(
                ErrorCode::IO,
                format!("file not found: {}", file.display()),
            )
            .with_remediation("check the path, or resolve it against the command workspace"));
        }

        let mut buffers = self.lock();

        let existing_id = buffers.iter().find_map(|(id, buffer)| {
            buffer
                .path()
                .is_some_and(|path| path == file)
                .then_some(*id)
        });
        if let Some(id) = existing_id {
            if let Some(buffer) = buffers.get_mut(&id) {
                let summary = BufferSummary {
                    id,
                    file: Some(file.clone()),
                    len_chars: buffer.len_chars(),
                    len_lines: buffer.len_lines(),
                    dirty: buffer.is_dirty(),
                };
                return Ok(summary);
            }
            // Impossible entre deux accès sous le même verrou : on retombe
            // sur une ouverture standard plutôt que de paniquer.
        }

        let buffer = TextBuffer::open(&file)?;
        let id = self.allocate(&mut buffers);
        let summary = BufferSummary {
            id,
            file: Some(file.clone()),
            len_chars: buffer.len_chars(),
            len_lines: buffer.len_lines(),
            dirty: buffer.is_dirty(),
        };
        buffers.insert(id, buffer);

        Ok(summary)
    }

    /// Texte intégral du buffer.
    pub fn text(&self, id: BufferId) -> Result<String> {
        let buffers = self.lock();
        let buffer = buffer_of(&buffers, id)?;
        Ok(buffer.text())
    }

    /// Remplace une tranche via des positions ligne:colonne.
    ///
    /// Renvoie le caret appliqué (indices de caractères) et la position du
    /// carret après le texte inséré.
    pub fn edit(
        &self,
        id: BufferId,
        start: Position,
        end: Position,
        text: &str,
    ) -> Result<(Caret, Position)> {
        let mut buffers = self.lock();
        let buffer = buffer_mut(&mut buffers, id)?;

        let head = start.to_char(buffer.rope())?;
        let tail = end.to_char(buffer.rope())?;
        let caret = buffer.edit(CharSpan::new(head, tail), text)?;

        let paste_position =
            Position::from_char(buffer.rope(), caret.start + caret.inserted.chars().count())?;

        Ok((caret, paste_position))
    }

    /// Annule la dernière transaction du buffer ; renvoie false si vide.
    pub fn undo(&self, id: BufferId) -> Result<bool> {
        let mut buffers = self.lock();
        Ok(buffer_mut(&mut buffers, id)?.undo())
    }

    /// Rejoue la dernière transaction annulée ; renvoie false si rien à rejouer.
    pub fn redo(&self, id: BufferId) -> Result<bool> {
        let mut buffers = self.lock();
        Ok(buffer_mut(&mut buffers, id)?.redo())
    }

    /// Persiste le buffer sur disque.
    pub fn save(&self, id: BufferId) -> Result<BufferSummary> {
        let mut buffers = self.lock();
        let buffer = buffer_mut(&mut buffers, id)?;
        buffer.save()?;
        Ok(Self::summary(
            id,
            buffer.path().map(Path::to_path_buf),
            buffer,
        ))
    }

    /// Recherche depuis un indice, avec un plafond de résultats.
    ///
    /// Défense contre la consommation de ressource (`OWASP API #4`) : jamais
    /// plus de `limit` occurrences sont converties en claires positions.
    pub fn find(
        &self,
        id: BufferId,
        needle: &str,
        from: usize,
        limit: usize,
    ) -> Result<Vec<MatchSpan>> {
        let mut buffers = self.lock();
        let buffer = buffer_mut(&mut buffers, id)?;

        let mut matches = Vec::new();
        let mut cursor = from;

        while matches.len() < limit {
            let Some(span) = buffer.find(needle, cursor)? else {
                return Ok(matches);
            };

            let position = Position::from_char(buffer.rope(), span.start)?;
            matches.push(MatchSpan {
                start: span.start,
                end: span.end,
                line: position.line,
                col: position.col,
            });
            cursor = span.start + 1;
        }

        Ok(matches)
    }

    /// Ferme un buffer ; refuse sans `discard` si le buffer est sale.
    pub fn close(&self, id: BufferId, discard: bool) -> Result<BufferSummary> {
        let mut buffers = self.lock();

        let summary = {
            let buffer = buffer_mut(&mut buffers, id)?;
            if buffer.is_dirty() && !discard {
                return Err(PlsError::app(
                    ErrorCode::COMMAND,
                    format!("buffer {id} has unsaved changes"),
                )
                .with_remediation("call editor.save first, or pass `discard: true`"));
            }
            Self::summary(id, buffer.path().map(Path::to_path_buf), buffer)
        };

        buffers.remove(&id);
        Ok(summary)
    }

    /// Tous les buffers ouverts, par identifiant croissant.
    pub fn list(&self) -> Vec<BufferSummary> {
        self.lock()
            .iter()
            .map(|(id, buffer)| Self::summary(*id, buffer.path().map(Path::to_path_buf), buffer))
            .collect()
    }

    fn allocate(&self, buffers: &mut BTreeMap<BufferId, TextBuffer>) -> BufferId {
        let mut raw = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        while buffers.contains_key(&BufferId::new(raw)) {
            raw += 1;
        }

        BufferId::new(raw)
    }

    fn summary(id: BufferId, file: Option<PathBuf>, buffer: &TextBuffer) -> BufferSummary {
        BufferSummary {
            id,
            file,
            len_chars: buffer.len_chars(),
            len_lines: buffer.len_lines(),
            dirty: buffer.is_dirty(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<BufferId, TextBuffer>> {
        // Un panique dans une section critique ne doit pas contaminer la
        // table : on récupère les données le plus intact possible.
        self.buffers.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn buffer_of(buffers: &BTreeMap<BufferId, TextBuffer>, id: BufferId) -> Result<&TextBuffer> {
    buffers.get(&id).ok_or_else(|| unknown_buffer(id))
}

fn buffer_mut(
    buffers: &mut BTreeMap<BufferId, TextBuffer>,
    id: BufferId,
) -> Result<&mut TextBuffer> {
    buffers.get_mut(&id).ok_or_else(|| unknown_buffer(id))
}

fn unknown_buffer(id: BufferId) -> PlsError {
    PlsError::app(ErrorCode::COMMAND, format!("unknown buffer {id}"))
        .with_remediation("list opened buffers with `editor.list`")
}
