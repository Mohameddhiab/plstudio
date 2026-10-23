//! Commandes `editor.*` : ouverture, lecture, édition, undo/redo,
//! recherche, sauvegarde et fermeture des buffers.
//!
//! Ces commandes sont la face observable du store d'édition ; elles valident
//! leurs paramètres via `parse_params` et délèguent tout le travail à
//! [`EditorStore`]. `editor.edit` est marqué `undoable` : le moteur d'undo
//! de `pls-text` fournit l'inverse, `editor.undo` l'expose.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use pls_base::parse_params;
use pls_base::{BufferId, Command, CommandCtx, CommandMeta, CommandOutput, DangerLevel, Result};
use pls_text::Position;
use serde::Deserialize;
use serde_json::Value;

use crate::Engine;
use crate::editor::EditorStore;

/// Enregistre les commandes d'édition sur le moteur en construction.
///
/// Les commandes partagent un seul `EditorStore` (un seul espace de buffers
/// par moteur).
pub(crate) fn register(engine: &mut Engine) {
    let editor = Arc::new(EditorStore::new());
    engine.register(OpenCommand::new(editor.clone()));
    engine.register(GetCommand::new(editor.clone()));
    engine.register(EditCommand::new(editor.clone()));
    engine.register(UndoCommand::new(editor.clone()));
    engine.register(RedoCommand::new(editor.clone()));
    engine.register(SaveCommand::new(editor.clone()));
    engine.register(FindCommand::new(editor.clone()));
    engine.register(ListCommand::new(editor.clone()));
    engine.register(CloseCommand::new(editor));
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenParams {
    file: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetParams {
    id: BufferId,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPosition {
    line: usize,
    col: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditParams {
    id: BufferId,
    start: RawPosition,
    end: RawPosition,
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BufferParams {
    id: BufferId,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FindParams {
    id: BufferId,
    needle: String,
    from: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CloseParams {
    id: BufferId,
    discard: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {}

/// Résout un chemin de paramètre contre le workspace de la commande.
pub(crate) fn resolve_path(file: &str, workspace: &Path) -> std::path::PathBuf {
    let path = Path::new(file);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace.join(path)
    }
}

fn position(raw: &RawPosition) -> Position {
    Position {
        line: raw.line,
        col: raw.col,
    }
}

/// `editor.open` — ouvre un fichier, ou réutilise son buffer existant.
pub struct OpenCommand {
    editor: Arc<EditorStore>,
}

impl OpenCommand {
    pub fn new(editor: Arc<EditorStore>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Command for OpenCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("editor.open", "Open a file", "editor", DangerLevel::Read)
            .with_description(
                "Ouvre un fichier dans un buffer du moteur. Un fichier déjà ouvert \
                 réutilise son buffer en mémoire (les modifications locales font foi).",
            )
    }

    async fn run(&self, ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: OpenParams = parse_params(&params)?;
        let path = resolve_path(&params.file, &ctx.workspace);
        let summary = self.editor.open(&path)?;

        Ok(CommandOutput::json(serde_json::json!(summary)))
    }
}

/// `editor.get` — texte intégral d'un buffer ouvert.
pub struct GetCommand {
    editor: Arc<EditorStore>,
}

impl GetCommand {
    pub fn new(editor: Arc<EditorStore>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Command for GetCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("editor.get", "Get buffer text", "editor", DangerLevel::Read)
            .with_description(
                "Renvoie le texte complet d'un buffer ouvert, avec son résumé \
                 (chemin, nombre de caractères, drapeau dirty).",
            )
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: GetParams = parse_params(&params)?;
        let text = self.editor.text(params.id)?;
        let summary = self
            .editor
            .list()
            .into_iter()
            .find(|summary| summary.id == params.id);

        Ok(CommandOutput::json(serde_json::json!({
            "summary": summary,
            "text": text,
        })))
    }
}

/// `editor.edit` — remplace une tranche via des positions ligne:colonne.
pub struct EditCommand {
    editor: Arc<EditorStore>,
}

impl EditCommand {
    pub fn new(editor: Arc<EditorStore>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Command for EditCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("editor.edit", "Edit buffer", "editor", DangerLevel::Write)
            .undoable(true)
            .with_description(
                "Remplace la tranche [start, end) par `text` dans le buffer. Positions \
                 en ligne:colonne (caractères, hors terminateur). Annulable via \
                 `editor.undo`.",
            )
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: EditParams = parse_params(&params)?;
        let (caret, paste_position) = self.editor.edit(
            params.id,
            position(&params.start),
            position(&params.end),
            &params.text,
        )?;

        Ok(CommandOutput::json(serde_json::json!({
            "caret": caret,
            "position": paste_position,
        })))
    }
}

/// `editor.undo` — annule la dernière transaction du buffer.
pub struct UndoCommand {
    editor: Arc<EditorStore>,
}

impl UndoCommand {
    pub fn new(editor: Arc<EditorStore>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Command for UndoCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new(
            "editor.undo",
            "Undo buffer change",
            "editor",
            DangerLevel::Write,
        )
        .with_description(
            "Annule le dernier pas d'undo du buffer. Renvoie `undone: false` quand \
                 l'historique est vide (ce n'est pas une erreur).",
        )
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: BufferParams = parse_params(&params)?;
        let undone = self.editor.undo(params.id)?;

        Ok(CommandOutput::json(serde_json::json!({ "undone": undone })))
    }
}

/// `editor.redo` — rejoue la dernière transaction annulée.
pub struct RedoCommand {
    editor: Arc<EditorStore>,
}

impl RedoCommand {
    pub fn new(editor: Arc<EditorStore>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Command for RedoCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new(
            "editor.redo",
            "Redo buffer change",
            "editor",
            DangerLevel::Write,
        )
        .with_description(
            "Rejoue le dernier pas annulé du buffer. Renvoie `redone: false` quand \
                 rien n'est rejouable.",
        )
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: BufferParams = parse_params(&params)?;
        let redone = self.editor.redo(params.id)?;

        Ok(CommandOutput::json(serde_json::json!({ "redone": redone })))
    }
}

/// `editor.save` — persiste le buffer sur disque.
pub struct SaveCommand {
    editor: Arc<EditorStore>,
}

impl SaveCommand {
    pub fn new(editor: Arc<EditorStore>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Command for SaveCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("editor.save", "Save buffer", "editor", DangerLevel::Write)
            .with_description(
                "Écrit le contenu du buffer dans son fichier. Opération externe \
                 journalisée ; le buffer repasse à `dirty: false`.",
            )
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: BufferParams = parse_params(&params)?;
        let summary = self.editor.save(params.id)?;

        Ok(CommandOutput::json(serde_json::json!(summary)))
    }
}

/// `editor.find` — occurrences dans un buffer, bornées par `limit`.
pub struct FindCommand {
    editor: Arc<EditorStore>,
}

impl FindCommand {
    pub fn new(editor: Arc<EditorStore>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Command for FindCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("editor.find", "Find in buffer", "editor", DangerLevel::Read)
            .with_description(
                "Recherche une sous-chaîne dans le buffer. `from` est un indice de \
                 caractère, `limit` borne le nombre de résultats (défaut 200, max \
                 1000) ; `truncated` signale les résultats coupés.",
            )
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: FindParams = parse_params(&params)?;
        let from = params.from.unwrap_or(0);
        let limit = params.limit.unwrap_or(200).clamp(1, 1000);
        let matches = self.editor.find(params.id, &params.needle, from, limit)?;

        Ok(CommandOutput::json(serde_json::json!({
            "matches": matches,
            "truncated": matches.len() >= limit,
        })))
    }
}

/// `editor.list` — buffers ouverts.
pub struct ListCommand {
    editor: Arc<EditorStore>,
}

impl ListCommand {
    pub fn new(editor: Arc<EditorStore>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Command for ListCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new(
            "editor.list",
            "List opened buffers",
            "editor",
            DangerLevel::Read,
        )
        .with_description(
            "Liste les buffers ouverts du moteur : identifiant, fichier, \
                 longueurs, drapeau dirty.",
        )
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let _params: ListParams = parse_params(&params)?;

        Ok(CommandOutput::json(serde_json::json!({
            "buffers": self.editor.list(),
        })))
    }
}

/// `editor.close` — ferme un buffer ; refuse sans `discard` s'il est sale.
pub struct CloseCommand {
    editor: Arc<EditorStore>,
}

impl CloseCommand {
    pub fn new(editor: Arc<EditorStore>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Command for CloseCommand {
    fn meta(&self) -> CommandMeta {
        CommandMeta::new("editor.close", "Close buffer", "editor", DangerLevel::Write)
            .with_description(
                "Retire un buffer de la mémoire du moteur. Si le buffer contient \
                 des modifications non sauvegardées, la commande échoue sauf si \
                 `discard: true` est passé explicitement.",
            )
    }

    async fn run(&self, _ctx: &CommandCtx, params: Value) -> Result<CommandOutput> {
        let params: CloseParams = parse_params(&params)?;
        let summary = self
            .editor
            .close(params.id, params.discard.unwrap_or(false))?;

        Ok(CommandOutput::json(serde_json::json!(summary)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;
    use pls_base::{CommandId, ErrorCode, TrustLevel};
    use serde_json::{Value, json};

    /// Exécute une commande par identifiant, avec un workspace de test.
    async fn run(engine: &Engine, id: &str, params: Value) -> Value {
        let ctx = CommandCtx::new(engine.next_job(), ".", TrustLevel::Trusted);
        engine
            .execute(&CommandId::from(id), &ctx, params)
            .await
            .expect("command succeeds")
            .value
    }

    async fn execute_raw(engine: &Engine, id: &str, params: Value) -> Result<Value> {
        let ctx = CommandCtx::new(engine.next_job(), ".", TrustLevel::Trusted);
        engine
            .execute(&CommandId::from(id), &ctx, params)
            .await
            .map(|output| output.value)
    }

    fn json_id(value: &Value) -> u64 {
        value.get("id").and_then(Value::as_u64).expect("buffer id")
    }

    fn text_of(value: &Value) -> &str {
        value.get("text").and_then(Value::as_str).expect("text")
    }

    #[tokio::test]
    async fn open_edit_undo_redo_save_close_roundtrip() {
        let directory = tempfile::tempdir().expect("temp dir");
        let file = directory.path().join("main.dart");
        std::fs::write(&file, "void main() {}\n").expect("fixture");

        let engine = Engine::new();
        let opened = run(
            &engine,
            "editor.open",
            json!({ "file": file.to_string_lossy() }),
        )
        .await;
        // "void main() {}\n" : la ligne vide après le \n final compte.
        assert_eq!(opened.get("len_lines").and_then(Value::as_u64), Some(2));
        let id = json_id(&opened);

        // Remplace "void" par "fn" sur la ligne 0.
        let edited = run(
            &engine,
            "editor.edit",
            json!({
                "id": id,
                "start": { "line": 0, "col": 0 },
                "end": { "line": 0, "col": 4 },
                "text": "fn",
            }),
        )
        .await;
        let position = edited.get("position").expect("position");
        assert_eq!(position.get("line").and_then(Value::as_u64), Some(0));
        assert_eq!(position.get("col").and_then(Value::as_u64), Some(2));

        let text = run(&engine, "editor.get", json!({ "id": id })).await;
        assert_eq!(text_of(&text), "fn main() {}\n");

        let undone = run(&engine, "editor.undo", json!({ "id": id })).await;
        assert_eq!(undone.get("undone").and_then(Value::as_bool), Some(true));
        let text = run(&engine, "editor.get", json!({ "id": id })).await;
        assert_eq!(text_of(&text), "void main() {}\n");

        let redone = run(&engine, "editor.redo", json!({ "id": id })).await;
        assert_eq!(redone.get("redone").and_then(Value::as_bool), Some(true));

        run(&engine, "editor.save", json!({ "id": id })).await;
        assert_eq!(
            std::fs::read_to_string(&file).expect("saved file"),
            "fn main() {}\n"
        );

        let closed = run(&engine, "editor.close", json!({ "id": id })).await;
        assert_eq!(closed.get("id").and_then(Value::as_u64), Some(id));

        let error = execute_raw(&engine, "editor.get", json!({ "id": id }))
            .await
            .expect_err("buffer is gone");
        assert_eq!(error.code(), ErrorCode::COMMAND);
    }

    #[tokio::test]
    async fn close_refuses_a_dirty_buffer_without_discard() {
        let directory = tempfile::tempdir().expect("temp dir");
        let file = directory.path().join("util.dart");
        std::fs::write(&file, "// code\n").expect("fixture");

        let engine = Engine::new();
        let opened = run(
            &engine,
            "editor.open",
            json!({ "file": file.to_string_lossy() }),
        )
        .await;
        let id = json_id(&opened);

        run(
            &engine,
            "editor.edit",
            json!({
                "id": id,
                "start": { "line": 1, "col": 0 },
                "end": { "line": 1, "col": 0 },
                "text": "// more\n",
            }),
        )
        .await;

        let error = execute_raw(&engine, "editor.close", json!({ "id": id }))
            .await
            .expect_err("dirty buffer");
        assert_eq!(error.code(), ErrorCode::COMMAND);
        assert!(error.to_string().contains("unsaved"));

        // Le disque n'a pas été touché par le refus.
        assert_eq!(
            std::fs::read_to_string(&file).expect("file untouched"),
            "// code\n"
        );

        run(
            &engine,
            "editor.close",
            json!({ "id": id, "discard": true }),
        )
        .await;
        let buffers = run(&engine, "editor.list", json!({})).await;
        assert_eq!(
            buffers
                .get("buffers")
                .and_then(Value::as_array)
                .expect("buffers array")
                .len(),
            0
        );
    }

    #[tokio::test]
    async fn find_is_bounded_by_limit() {
        let directory = tempfile::tempdir().expect("temp dir");
        let file = directory.path().join("loops.dart");
        std::fs::write(&file, "aa\naa\naa\n").expect("fixture");

        let engine = Engine::new();
        let opened = run(
            &engine,
            "editor.open",
            json!({ "file": file.to_string_lossy() }),
        )
        .await;
        let id = json_id(&opened);

        let found = run(
            &engine,
            "editor.find",
            json!({ "id": id, "needle": "a", "limit": 2 }),
        )
        .await;
        let matches = found
            .get("matches")
            .and_then(Value::as_array)
            .expect("matches array");
        assert_eq!(matches.len(), 2);
        assert_eq!(found.get("truncated").and_then(Value::as_bool), Some(true));
        assert_eq!(matches[0].get("line").and_then(Value::as_u64), Some(0));
    }

    #[tokio::test]
    async fn reopening_a_file_reuses_its_buffer() {
        let directory = tempfile::tempdir().expect("temp dir");
        let file = directory.path().join("reuse.dart");
        std::fs::write(&file, "class A {}\n").expect("fixture");

        let engine = Engine::new();
        let first = run(
            &engine,
            "editor.open",
            json!({ "file": file.to_string_lossy() }),
        )
        .await;
        let id = json_id(&first);

        run(
            &engine,
            "editor.edit",
            json!({
                "id": id,
                "start": { "line": 0, "col": 6 },
                "end": { "line": 0, "col": 7 },
                "text": "B",
            }),
        )
        .await;

        let second = run(
            &engine,
            "editor.open",
            json!({ "file": file.to_string_lossy() }),
        )
        .await;
        assert_eq!(json_id(&second), id);
        assert!(second.get("dirty").and_then(Value::as_bool).unwrap_or(true));
        // L'édition locale reste la vérité en mémoire.
        let text = run(&engine, "editor.get", json!({ "id": id })).await;
        assert_eq!(text_of(&text), "class B {}\n");
    }

    #[tokio::test]
    async fn bad_positions_are_invalid_params() {
        let directory = tempfile::tempdir().expect("temp dir");
        let file = directory.path().join("small.dart");
        std::fs::write(&file, "x\n").expect("fixture");

        let engine = Engine::new();
        let opened = run(
            &engine,
            "editor.open",
            json!({ "file": file.to_string_lossy() }),
        )
        .await;
        let id = json_id(&opened);

        let error = execute_raw(
            &engine,
            "editor.edit",
            json!({
                "id": id,
                "start": { "line": 5, "col": 0 },
                "end": { "line": 5, "col": 1 },
                "text": "y",
            }),
        )
        .await
        .expect_err("line out of document");
        assert_eq!(error.code(), ErrorCode::INVALID_PARAMS);

        let error = execute_raw(&engine, "editor.find", json!({ "id": id, "needle": "" }))
            .await
            .expect_err("empty needle");
        assert_eq!(error.code(), ErrorCode::INVALID_PARAMS);
    }
}
