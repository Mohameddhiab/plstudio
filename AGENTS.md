# AGENTS.md — règles pour les agents et contributeurs

Ce dépôt construit **plstudio**, un IDE Rust pour Flutter/Android. Lis d'abord :

- `plan.md` — périmètre, roadmap, liste de coupe.
- `architecture.md` — couches, crates, commandes, sécurité.
- `design.md` — UI/UX.
- `docs/adr/` — décisions structurantes.
- `docs/threat-model.md` — actifs, frontières de confiance, menaces.
- `docs/control-protocol.md`, `docs/mcp.md`, `docs/privacy.md` — contrats de sécurité.

## Règles de périmètre

- v0.1 : **Flutter + Android** uniquement. Pas de React Native, Kotlin, Java, iOS ou Swift dans le code de la v0.1.
- L'émulateur v0.1 utilise **scrcpy dans une fenêtre séparée**. Le flux intégré est v0.2.
- L'inspecteur v0.1 fait **pick → fichier:ligne + Wrap with**. Pas de glisser-déposer ni d'édition complète des propriétés.
- Le thème v0.1 est **sombre uniquement**. FR/EN. Arabe/RTL en v2.
- Toute dérive se règle par la liste de coupe, jamais en ajoutant une fonctionnalité.

## Méthode de travail

- Toute modification, même mineure, doit devenir un **commit**.
- Chaque commit est **push** sur `main` après validation des checks.
- L'agent indique explicitement si on continue ou pas après chaque commit.
- Un commit = une intention logique, pas une collection hétérogène.
- Convention : `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`.
- Avant un commit de code :
  - `cargo fmt --all`
  - `cargo xtask layers`
  - `cargo test --workspace`
- Pas de commit si les tests échouent.

## Architecture

- Respecte les couches de `architecture.md` :
  - 0 `pls-base`
  - 1 `pls-text`, `pls-sdk`
  - 2 `pls-avd`, `pls-project`
  - 3 `pls-lsp`, `pls-run`, `pls-emulator`, `pls-inspector`, `pls-agent`
  - 4 `pls-engine`
  - 5 `pls-control`, `pls-mcp`, `pls-ui`
  - 6 `plstudio`, `plstudio-cli`
- Une crate ne dépend que de couches strictement inférieures.
- Aucune dépendance entre crates de même couche. Les DTO partagés vont dans `pls-base`.
- Les couches basses ne remontent jamais vers l'engine.
- `pls-agent` définit `ToolHost` ; `pls-engine` l'implémente.
- `pls-inspector` émet une localisation ; il n'ouvre pas les fichiers.
- `pls-ui` et `pls-mcp` ne se connaissent pas ; les deux passent par `pls-engine`.
- `pls-testkit` est réservé aux tests.

## Commandes

- Toute action passe par `engine.execute` (UI, CLI, agent, MCP).
- `Command::run` est **async**, avec `CommandCtx`, annulation, progression et `JobId`.
- Les paramètres sont validés avant exécution.
- Les erreurs sont structurées : `{code, message, remediation, details}` ; codes `PLS-E-XXXX`.
- Les menus et la palette sont générés depuis le registre de commandes.

## Style Rust

- `cargo fmt --all` et `cargo clippy --workspace --all-targets -- -D warnings` doivent passer.
- Pas de `unwrap()`, `expect()`, `panic!` ou `unsafe` dans le code de production. Les tests peuvent utiliser `unwrap()` pour rester lisibles.
- `thiserror` dans les crates, `anyhow` uniquement dans les binaires (pas encore ajouté).
- Pas de `dbg!`, pas de code commenté, pas de TODO sans issue.
- Préfère les types explicites aux `String` non validés.
- Les commentaires expliquent le *pourquoi*, pas le *comment*.

## Tests

- Chaque crate a au moins un test unitaire dès qu'elle contient de la logique.
- Les processus externes passent par `ProcessRunner` ; utilise `pls-testkit`.
- Les fixtures vivent dans `pls-testkit` (NDJSON `flutter run`, logcat, `avdmanager`).
- `cargo xtask ci` doit passer avant toute PR.
- Nightly : E2E Windows avec émulateur (plus tard).

## Sécurité

- Workspace trust : pas de build automatique à l'ouverture d'un dépôt inconnu.
- Canal de contrôle : token par session, ACL, audit.
- MCP : read-only par défaut ; `write`/`destructive` exigent une confirmation.
- Agent : consentement avant envoi de code, `.plstudioignore`, redaction des secrets, pas d'auto-apply.
- Clés API dans le trousseau système ; jamais dans les logs.
- Pas de GPL/AGPL dans les dépendances. `cargo-deny` pour vérifier.

## Nommage

- Produit : `plstudio`.
- Crates : `pls-*`.
- Binaires : `plstudio`, `plstudio-cli` ; alias `pls`.
- Env : `PLS_*`. Erreurs : `PLS-E-*`.
- Config : `%APPDATA%\plstudio`, `~/.config/plstudio`.
- Données : `%LOCALAPPDATA%\plstudio`, `~/.local/share/plstudio`.

## Commandes utiles

```bash
cargo xtask ci
cargo xtask layers
cargo test --workspace
cargo run -p plstudio-cli -- commands
```
