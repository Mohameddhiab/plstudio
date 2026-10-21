# plstudio — Architecture

Voir aussi : [plan.md](plan.md), [design.md](design.md), [docs/adr/](docs/adr/).

## 1. Principes

1. **Workspace Cargo en couches strictes** : chaque crate ne dépend que des couches en dessous. Vérifié par `cargo xtask layers`.
2. **Moteur sans UI** : rien sous `pls-ui` ne connaît l'interface (egui, GPUI, webview ou autre).
3. **Tout est une commande** : id stable + paramètres JSON. Menu, raccourci, CLI, agent IA et MCP appellent `engine.execute`.
4. **Headless d'abord** : tout est utilisable via `plstudio-cli`.
5. **Piloter, ne pas réécrire** : LSP, SDK Android, Flutter, VM Service, scrcpy.
6. **Zéro panic** : erreurs retournées, pas de `unwrap()`/`expect()`/`panic!`/`unsafe` hors tests.
7. **Undo pour les documents et le projet** ; les opérations externes sont journalisées.
8. **Confiance par défaut** : workspace trust, token sur le canal de contrôle, danger levels MCP, consentement avant envoi de code à l'API.

## 2. Vue d'ensemble

```
┌──────────────────────── apps ──────────────────────────┐
│   plstudio (desktop)          plstudio-cli (headless)  │
└────────┬────────────────────────────────┬──────────────┘
         │                                │
    ┌────▼─────┐                    ┌─────▼─────┐
    │  pls-ui  │                    │  pls-mcp  │
    └────┬─────┘                    └─────┬─────┘
         └───────────────┬────────────────┘
                    ┌────▼─────┐
                    │pls-engine│  commandes, jobs, undo, événements
                    └────┬─────┘
     ┌──────┬────────┬───┴─────┬──────────┬────────┐
     │agent │  run   │inspector│ emulator │lsp     │
     └──┬───┴───┬────┴────┬────┴────┬─────┴───┬────┘
        └───────┴────┬────┴─────────┴─────────┘
               ┌─────▼──────┐
               │pls-project │  détection + modèle de projet
               └─────┬──────┘
        ┌────────┬───┴────┬───────────┐
      pls-avd  pls-text  pls-sdk   (couche 2/1)
        └────────┴────────┴───────────┘
               ┌────────────────────┐
               │      pls-base      │  erreurs, logs, process, settings, événements
               └────────────────────┘
```

## 3. Crates

| Couche | Crate | Rôle |
|---|---|---|
| 0 | `pls-base` | Erreurs, tracing, chemins, settings atomiques, exécution de processus, bus d'événements, types de commandes/événements, IDs, trust |
| 1 | `pls-text` | Rope (`ropey`), tree-sitter, buffers, undo/redo, encodages, fins de ligne |
| 1 | `pls-sdk` | Détection, téléchargement `cmdline-tools`, checksums, `sdkmanager`, licences |
| 2 | `pls-avd` | `avdmanager`, devices, `config.ini`, démarrage/arrêt, accélération |
| 2 | `pls-project` | Détection Flutter, modèle de projet, templates via `flutter create` |
| 3 | `pls-lsp` | Client LSP générique (Dart en v0.1, TypeScript/Kotlin/Java en v2) |
| 3 | `pls-run` | `flutter run --machine`, hot reload/restart, `adb`, logcat |
| 3 | `pls-emulator` | scrcpy (fenêtre séparée en v0.1), gRPC en secours, entrées, rotation, captures |
| 3 | `pls-inspector` | VM Service Flutter, widget tree, sélection, localisation, demande de wrap |
| 3 | `pls-agent` | Provider (Claude), contexte, `ToolHost`, boucle d'outils, diffs, budgets |
| 4 | `pls-engine` | Registre de commandes, jobs, annulation, progression, undo/audit, événements, orchestration, trust |
| 5 | `pls-mcp` | Serveur MCP (`rmcp`), outils depuis le registre, danger levels, audit |
| 5 | `pls-ui` | Interface (egui/GPUI/webview selon ADR 0001), panneaux, palette, inspecteur, agent |
| 6 | `plstudio` | Binaire desktop |
| 6 | `plstudio-cli` | Headless, JSON, `mcp`, `--control` |
| — | `xtask` | `ci`, `layers`, `package`, `schema` |
| — | `pls-testkit` | Fakes, fixtures, harnais de test (dev uniquement, hors couches) |

**Règles de couches** :

- Une crate de couche N dépend uniquement de couches < N.
- Aucune dépendance entre crates de même couche. Les DTO partagés vivent dans `pls-base`.
- Les couches basses ne remontent jamais vers l'engine. Elles retournent des données ou émettent des événements ; l'engine orchestre.
- `pls-inspector` émet une localisation (`file`, `line`, `widget`) ; il n'ouvre pas le fichier. C'est l'engine qui appelle `editor.open`.
- `pls-agent` définit le trait `ToolHost` ; `pls-engine` l'implémente. L'agent ne connaît ni l'engine ni l'UI.
- `pls-ui` et `pls-mcp` ne se connaissent pas. Les deux passent par `pls-engine`.
- `pls-testkit` est une dev-dependency, exclue du contrôle de couches.

## 4. Moteur, commandes et jobs

```rust
pub struct CommandId(&'static str);          // "avd.start"

#[async_trait]
pub trait Command: Send + Sync {
    fn id(&self) -> CommandId;
    fn meta(&self) -> CommandMeta;           // titre, catégorie, danger, undoable
    fn schema(&self) -> RootSchema;          // paramètres
    async fn run(&self, ctx: &CommandCtx, params: Value) -> Result<CommandOutput, CommandError>;
}
```

- Registre unique : `engine.commands` liste tout, avec métadonnées et JSON Schema.
- `engine.execute {command, params, request_id, wait}` est le seul point d'entrée.
- Les commandes longues retournent un `JobId` et streament la progression ; `wait: true` attend la fin.
- `engine.cancel {job_id}` annule via `CancellationToken`.
- `CommandCtx` fournit `JobCtx` : annulation, progression, bus d'événements, workspace, trust level.
- Les paramètres sont validés contre le schéma (`jsonschema`) avant exécution.
- Erreurs structurées : `{code, message, remediation, details}`. Codes stables `PLS-E-XXXX`.
- `thiserror` dans les crates, `anyhow` uniquement dans les binaires.
- Les menus de l'UI sont **générés** depuis le registre.

### Métadonnées de commande

| Champ | Rôle |
|---|---|
| `id`, `title`, `description`, `category` | Affichage, palette, MCP |
| `schema` | Validation et génération d'UI |
| `danger` | `read`, `write`, `destructive` |
| `requires_confirmation` | Confirmation UI ou MCP |
| `undoable` | Document/projet uniquement |
| `requires_ui` | Commande impossible en headless (dialogue) |
| `keybinding` | Raccourci par défaut |

### Événements

- Enveloppe : `{seq, ts, topic, payload}`.
- `watch` pour la dernière frame (émulateur) : on peut perdre les anciennes.
- `broadcast` pour les notifications (progression, état).
- `mpsc` borné + ring buffer avec numéros de séquence pour logcat : on ne perd pas de lignes silencieusement.
- Chaque topic documente sa politique de perte.

### Undo et audit

- **Undo** : mutations de documents et de projet. Les commandes retournent une `Mutation` (edits texte, écritures fichier) que l'engine empile.
- **Audit** : opérations externes (SDK, AVD, build, réseau) journalisées avec `job_id`, paramètres, résultat, durée.
- Les edits de l'agent forment une transaction : une seule étape d'undo.
- Une modification externe du fichier invalide l'undo du buffer concerné (watcher + hash).
- Redo vidé à la première nouvelle édition.

## 5. Commandes principales

```
sdk.detect  sdk.install  sdk.licenses
avd.list  avd.create  avd.delete  avd.start  avd.stop
project.open  project.create  project.detect
editor.open  editor.save  editor.applyEdit  editor.find
lsp.complete  lsp.definition  lsp.codeAction
run.start  run.stop  run.hotReload  run.hotRestart
logcat.start  logcat.filter  logcat.clear
emulator.open  emulator.close  emulator.rotate  emulator.screenshot
inspector.enableSelect  inspector.pick  inspector.wrap  inspector.properties
agent.ask  agent.applyDiff  agent.rejectDiff
ui.inspect  ui.screenshot  ui.clickWidget
engine.jobs  engine.cancel
```

Notes :

- `emulator.open` lance scrcpy dans une fenêtre séparée (v0.1) ; `emulator.close` ferme scrcpy ; `avd.stop` arrête l'émulateur. Le flux intégré est en v0.2.
- `inspector.enableSelect` active le mode sélection du Widget Inspector dans l'app.
- `inspector.pick` reçoit la sélection envoyée par l'app via le VM Service ; il ne fait pas de hit-test sur une capture.
- `inspector.wrap` demande une code action Dart LSP, prévisualise le diff, applique, puis déclenche `run.hotReload`.
- `ui.*` cible l'UI de l'IDE, pas la fenêtre scrcpy.

## 6. Canal de contrôle et MCP

- **Canal JSON-lines** : named pipe/UDS avec ACL, ou TCP local + token aléatoire par session. Handshake de version. Audit des appels.
- **MCP** (`plstudio-cli mcp`) : headless ou connecté à l'app en cours. Outils : `command_line`, `execute`, `inspect_project`, `screenshot`, `ui_click`.
- Chaque outil MCP hérite du `danger` de la commande. Read-only par défaut ; `write`/`destructive` exigent un flag ou une confirmation.
- Token, rate limit, audit log.
- Chaque widget de l'IDE a un id stable (`button:run`, `panel:logcat`) pour `ui.clickWidget`, `ui.inspect`, `ui.screenshot`.
- La fenêtre scrcpy est externe : l'inspecteur ne s'appuie pas sur `ui.clickWidget`, mais sur le mode sélection de l'app.

## 7. Flux clés

### 7.1 Installer le SDK et créer un émulateur

```
UI/CLI/MCP → sdk.detect (SDK, JDK, Node, Git, disque, accélération)
          → sdk.install → téléchargement (reprise, checksums, progression)
          → sdk.licenses (confirmation utilisateur)
          → avd.create {device, api, image, arch} → avd.start → emulator.open
```

Si aucun JDK n'est trouvé : proposition d'installer Eclipse Temurin dans un dossier utilisateur, avec consentement. Pas de JRE bundlé.

### 7.2 Build et run (Flutter)

```
run.start → pls-project détecte pubspec.yaml → flutter run --machine
         → événements (logs, VM Service URI)
         → pls-inspector se connecte
         → emulator.open (scrcpy) si demandé
```

### 7.3 Clic sur l'émulateur → code

```
inspector.enableSelect → mode sélection dans l'app
utilisateur clique dans la fenêtre scrcpy
  → Widget Inspector de l'app sélectionne le widget
  → VM Service renvoie widget + création (fichier:ligne)
  → pls-inspector émet une localisation
  → engine appelle editor.open {file, line}
```

La surbrillance est dessinée par l'app, pas par l'IDE.

### 7.4 Modification visuelle

```
inspector.wrap {widget:"Padding"} → lsp.codeAction "Wrap with Padding"
  → diff de prévisualisation → editor.applyEdit → run.hotReload → undo groupé
```

### 7.5 Agent IA

```
agent.ask → contexte (fichiers, erreurs, logcat) → API Claude
  → ToolHost (lire, éditer, build, logcat) → diff proposé
  → l'utilisateur valide → agent.applyDiff → build
  → si échec, l'agent reçoit l'erreur et propose un correctif (boucle bornée, optionnelle)
```

## 8. Choix techniques

| Domaine | Choix |
|---|---|
| Async | `tokio`, `tokio-util` (CancellationToken) |
| Texte | `ropey`, `tree-sitter` |
| LSP | `lsp-types`, client JSON-RPC maison ou `async-lsp` |
| Émulateur | scrcpy (fenêtre séparée en v0.1) ; gRPC `tonic`/`prost` en secours |
| WebSocket (VM Service) | `tokio-tungstenite` |
| HTTP (SDK, API Claude) | `reqwest` |
| JSON | `serde`, `serde_json`, `schemars`, `jsonschema` |
| Logs | `tracing`, `tracing-subscriber`, `tracing-appender` |
| Terminal | `portable-pty`, `alacritty_terminal` ou `vte` |
| Fichiers | `notify`, `directories`, `tempfile` |
| Recherche | `ignore`, `grep-searcher` |
| Diffs | `similar` |
| Secrets | `keyring` |
| MCP | `rmcp` |
| UI (à décider) | `egui` + `eframe` + `wgpu`, GPUI, ou webview + Monaco/CodeMirror |
| Build/CI | `xtask`, GitHub Actions |
| Packaging | `cargo-wix`/`cargo-dist` (.msi), AppImage, `.deb` |
| Supply chain | `cargo-deny`, `cargo-about`, `cargo-sbom` |

## 9. Décision UI

- **Candidats** : egui + éditeur maison, GPUI (vérifier la licence de la crate ; l'éditeur Zed reste GPL-3), webview + Monaco/CodeMirror.
- **Critères** : IME français, 60 fps sur 5 000 lignes, RAM, coût en semaines, licence compatible, accessibilité, RTL en v2.
- **Date limite** : fin S3. ADR 0001 signé.
- **Défaut** : egui si aucun candidat ne se détache.
- **Réversibilité** : le moteur ne dépend d'aucune UI ; `pls-ui` isole le choix.

## 10. Structure du dépôt

```
plstudio/
├─ apps/
│  ├─ plstudio/          # binaire desktop
│  └─ plstudio-cli/      # headless, MCP
├─ crates/
│  ├─ pls-base/  pls-text/  pls-sdk/  pls-avd/
│  ├─ pls-project/  pls-lsp/  pls-run/
│  ├─ pls-emulator/  pls-inspector/  pls-agent/
│  ├─ pls-engine/  pls-mcp/  pls-ui/
├─ xtask/
├─ docs/  (plan.md, architecture.md, design.md, control-protocol.md, mcp.md, privacy.md)
├─ docs/adr/  (0001-ui.md, 0002-emulator-streaming.md, 0003-code-editing.md, 0004-ios-swift.md)
├─ assets/  (icônes, ATTRIBUTION.md)
├─ packaging/
├─ .github/workflows/
├─ AGENTS.md  CLAUDE.md  ROADMAP.md  README.md
├─ LICENSE-MIT  LICENSE-APACHE  NOTICE
└─ Cargo.toml  clippy.toml  rustfmt.toml  rust-toolchain.toml
```

## 11. Sécurité et robustesse

- **Workspace trust** : un dépôt inconnu s'ouvre en mode restreint. Pas de build, pas de Gradle/npm, pas d'agent automatique avant confirmation.
- **Canal de contrôle** : named pipe/UDS avec ACL, ou TCP + token par session. Handshake de version, audit.
- **MCP** : read-only par défaut, danger levels, confirmations, token, rate limit.
- **Agent IA** : consentement explicite avant envoi de code, `.plstudioignore`, redaction des secrets, budget de tokens, pas d'auto-apply.
- **Clé API** : trousseau système, jamais dans les logs ; fallback fichier `0600` avec avertissement.
- **Processus enfants** : Job Objects sur Windows, groupes de processus sur Linux. Ne pas tuer le serveur adb de l'utilisateur.
- **Téléchargements** : HTTPS, checksums, reprise, proxy, espace disque.
- **Supply chain** : `cargo-deny`, SBOM, installateurs signés.
- **Licences** : MIT/Apache pour le projet ; allowlist `cargo-deny` ; pas de GPL/AGPL ; scrcpy Apache-2.0 ; FFmpeg LGPL/GPL et brevets H.264 à vérifier si utilisé.

## 12. Tests

- **Unitaires** par crate.
- **Intégration headless** : `plstudio-cli run --cmd 'sdk.detect'`, `avd.create`, `run.start` sur un projet témoin.
- **Fakes** : `pls-testkit` fournit `FakeProcessRunner`, `FakeAdb`, `FakeEmulator`, `FakeLsp`, `FakeClock`, `FakeFs`, `FakeAgentProvider`.
- **Fixtures** : sorties `flutter run --machine`, logcat `threadtime`, `avdmanager list device`, `sdkmanager --list`, widget tree VM Service.
- **E2E nightly** : runner Windows self-hosted avec émulateur ; création AVD, run, capture, sélection, assertion.
- **Snapshots UI** : `egui_kittest` si egui ; Playwright si webview.
- **Perf** : `criterion` pour rope, coloration, layout, démarrage ; tendance suivie.
- **Fuzz** : logcat, NDJSON, protocole de contrôle, settings.
- **CI** : `cargo xtask ci` = fmt, clippy, tests, layers, deny, build Windows et Linux.

## 13. Horizon iOS / Swift

- **v2 macOS** : iOS Simulator via Xcode/CoreSimulator (`xcrun simctl`), Swift via Xcode toolchain.
- **Swift sur Windows/Linux** : SourceKit-LSP, SwiftPM, `swift-format`, LLDB. Édition/compilation de cibles serveur/CLI, sans build iOS.
- **Windows/Linux → iOS** : uniquement via un hôte macOS distant (Mac physique, CI, cloud) en v2.5.
- **Interdit** : iOS Simulator sur Windows/Linux, virtualisation de macOS sur du matériel non-Apple (EULA Apple), réutilisation de l'émulateur Android.
- Voir ADR 0004.

## 14. Conventions de nommage

- Produit : **plstudio**.
- Crates : préfixe **`pls-`** ; binaires `plstudio` et `plstudio-cli` ; alias CLI optionnel `pls`.
- Variables d'environnement : `PLS_CONFIG_DIR`, `PLS_DATA_DIR`, `PLS_LOG_DIR`.
- Codes d'erreur : `PLS-E-XXXX`.
- Config : `%APPDATA%\plstudio` (Windows), `~/.config/plstudio` (Linux).
- Données : `%LOCALAPPDATA%\plstudio`, `~/.local/share/plstudio`.
- Logs : `.../plstudio/logs/plstudio.log`.
