# plstudio

[![CI](https://github.com/Mohameddhiab/plstudio/actions/workflows/ci.yml/badge.svg)](https://github.com/Mohameddhiab/plstudio/actions/workflows/ci.yml)

IDE open source en Rust pour développer des applications Android avec **Flutter**. Rapide, léger, avec émulateur contrôlé par l'IDE, inspecteur visuel et agent IA.

Dépôt : <https://github.com/Mohameddhiab/plstudio>.

> **Statut : pré-alpha — Phase 0.** Le workspace Cargo, la CI, les crates socles et les ADR existent. L'UI n'est pas encore choisie (ADR 0001). Aucune release n'est publiée.

## Pourquoi plstudio

- **Rapide et léger** : démarrage < 2 s, faible consommation de RAM.
- **Flutter d'abord** : un seul outil pour éditer, builder, lancer et inspecter une app Android.
- **Émulateur au centre** : l'IDE contrôle le SDK, les AVD, adb et la rotation ; scrcpy s'ouvre dans une fenêtre séparée en v0.1, le flux intégré arrive en v0.2.
- **Mode visuel** : cliquer dans l'app pour sélectionner un widget, obtenir `fichier:ligne` et appliquer « Wrap with ».
- **Agent IA** : chat, contexte, diffs à valider, correction d'erreur de build en option.
- **Pilotable** : CLI headless et serveur MCP pour les agents.

## Périmètre v0.1

Inclus :

- Éditeur de code (rope, tree-sitter, LSP Dart), onglets, recherche, terminal.
- Détection/installation du SDK Android, gestion des licences, création et démarrage d'AVD.
- `flutter run`, hot reload, hot restart, logcat filtrable, erreurs cliquables.
- Panneau émulateur + scrcpy en fenêtre séparée ; rotation, captures, contrôles.
- Inspecteur : pick → `fichier:ligne`, propriétés read-only, « Wrap with ».
- Agent IA : chat, diffs, apply/reject ; auto-fix de build optionnel.
- MCP read-only + run ; CLI headless.
- Windows d'abord, Linux ensuite ; français/anglais ; thème sombre.

Hors v0.1 :

- React Native, Kotlin, Java (v2).
- iOS, Swift, macOS (v2). Swift sur Windows/Linux via SourceKit-LSP, sans build iOS.
- Arabe/RTL (v2). Thème clair/contraste élevé (v0.2).
- Glisser-déposer et édition complète des propriétés (v2).
- Debugger DAP, profiler, Compose Preview (v2).
- Multi-fenêtre, auto-update (v0.1.1).

## Documentation

- [plan.md](plan.md) — vision, périmètre, roadmap 30-36 semaines, risques.
- [architecture.md](architecture.md) — couches, crates, commandes, flux, sécurité.
- [design.md](design.md) — UI/UX, raccourcis, thèmes, accessibilité.
- [ROADMAP.md](ROADMAP.md) — résumé de la roadmap.
- [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md), [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
- [docs/control-protocol.md](docs/control-protocol.md), [docs/mcp.md](docs/mcp.md), [docs/privacy.md](docs/privacy.md), [docs/threat-model.md](docs/threat-model.md).
- [docs/adr/](docs/adr/) — décisions structurantes :
  - [0001-ui.md](docs/adr/0001-ui.md)
  - [0002-emulator-streaming.md](docs/adr/0002-emulator-streaming.md)
  - [0003-code-editing.md](docs/adr/0003-code-editing.md)
  - [0004-ios-swift.md](docs/adr/0004-ios-swift.md)

## Architecture en bref

Workspace Cargo en couches strictes :

```
pls-base
  ├─ pls-text, pls-sdk
  ├─ pls-avd, pls-project
  ├─ pls-lsp, pls-run, pls-emulator, pls-inspector, pls-agent
  ├─ pls-engine
  └─ pls-mcp, pls-ui
       └─ plstudio, plstudio-cli
```

- Le moteur ne dépend d'aucune UI.
- Toutes les actions passent par `engine.execute` (UI, CLI, agent, MCP).
- Les commandes sont asynchrones, annulables, avec progression et `JobId`.
- L'undo couvre les documents et le projet ; les opérations externes sont journalisées.
- `pls-agent` définit `ToolHost`, implémenté par `pls-engine`.
- `pls-inspector` émet une localisation ; il n'ouvre pas les fichiers.

## Roadmap

| Phase | Semaines | Livrable |
|---|---|---|
| 0. Fondations + spikes | 1-3 | Workspace, CI, ADR, décision UI |
| 1. Squelette headless + scrcpy | 4-8 | SDK/AVD/run/logcat + scrcpy |
| 2. Éditeur + LSP + émulateur | 9-16 | Éditeur, terminal, palette, settings, contrôles émulateur |
| 3. Inspecteur | 17-22 | Pick → code, Wrap with |
| 4. Agent + MCP | 23-26 | Diffs, MCP read-only |
| 5. Durcissement + release | 27-30 | MSI/AppImage/deb, docs |
| Buffer | 31-36 | Imprévus, polish |

## Prérequis (cible)

- Windows 11 (cible principale) ou Linux.
- Rust stable, Visual Studio Build Tools (C++), CMake/Ninja.
- JDK 17 minimum (21 recommandé) — détection `JAVA_HOME` → `PATH` → Android Studio → proposition Temurin.
- Flutter SDK et Android SDK.
- Git.
- Node LTS (utile pour les outils TypeScript et la v2).
- Virtualisation activée (WHPX/Hyper-V ou KVM) pour l'émulateur.

## Compiler

Le workspace est créé. L'UI réelle reste à choisir ; les binaires actuels sont des squelettes de Phase 0.

```bash
cargo xtask ci          # fmt, clippy, tests, layers
cargo run -p plstudio   # squelette desktop
cargo run -p plstudio-cli -- --help
cargo run -p plstudio-cli -- commands
```

## Contribuer

- Lis `CONTRIBUTING.md` et `AGENTS.md`.
- Ouvrir une issue pour discuter d'une fonctionnalité ou d'un bug.
- Les décisions structurantes passent par un ADR dans `docs/adr/`.
- Les contributions sont sous DCO (pas de CLA).

## Licence

MIT OR Apache-2.0. Voir `LICENSE-MIT` et `LICENSE-APACHE`.

## Remerciements

plstudio s'appuie sur Flutter, le SDK Android, scrcpy, ainsi que sur l'écosystème Rust (tokio, ropey, tree-sitter, egui/GPUI ou Monaco selon l'ADR 0001). Les polices Inter, JetBrains Mono et Noto Sans Arabic sont sous SIL OFL.
