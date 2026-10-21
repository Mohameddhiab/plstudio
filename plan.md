# plstudio — Plan du projet

IDE open source en Rust pour créer des applications mobiles Android avec Flutter, avec émulateur Android contrôlé par l'IDE (scrcpy), mode visuel et agent IA.

- **Licence** : MIT OR Apache-2.0
- **Plateformes** : Windows, Linux (macOS en v2)
- **Cible v0.1** : Android + Flutter uniquement
- **Durée** : 30-36 semaines (7-9 mois) jusqu'à la v0.1 publique
- **Dépôt** : `plstudio` — le handle GitHub `plstudio` est déjà pris ; utiliser un dépôt sous un compte personnel ou une organisation avec un handle disponible.

Voir aussi : [architecture.md](architecture.md), [design.md](design.md), [docs/adr/](docs/adr/).

---

## 1. Vision

Remplacer Android Studio pour le développement **Flutter sur Android** par un outil plus rapide et plus léger :

- Démarrage en moins de 2 secondes, faible consommation de RAM.
- Un seul outil pour éditer, builder, lancer et inspecter une app Flutter.
- Émulateur Android contrôlé par l'IDE, avec scrcpy dans une fenêtre séparée en v0.1, puis un flux intégré en v0.2.
- Mode visuel : cliquer dans l'app (via le Widget Inspector et scrcpy) pour trouver le widget et sa ligne de code.
- Agent IA intégré (chat, diffs à valider, correction des erreurs de build en option).
- Pilotable par agents IA via MCP, et en ligne de commande (headless).
- **Horizon iOS/Swift** : v2 sur macOS avec iOS Simulator et Swift ; support du langage Swift sur Windows/Linux via SourceKit-LSP, sans build iOS. Pas d'iOS Simulator sur Windows/Linux.

## 2. Périmètre

### Dans la v0.1

- Éditeur de code rapide (rope, tree-sitter, LSP Dart).
- Installation et gestion du SDK Android (`sdkmanager`) et des AVD (`avdmanager`).
- Build, run, hot reload, hot restart, logcat filtrable.
- Panneau émulateur : l'IDE contrôle le démarrage, adb, la rotation et les captures ; scrcpy s'ouvre dans sa propre fenêtre.
- Inspecteur visuel : clic dans l'app → widget → fichier:ligne, plus « Wrap with ».
- Agent IA : chat, contexte, diffs à valider.
- Serveur MCP read-only + commandes de run, et CLI headless.
- Windows d'abord, Linux ensuite.
- Français et anglais ; thème sombre uniquement.

### Hors v0.1

- React Native, Kotlin et Java (v2).
- iOS, Swift et macOS (v2). Le support du langage Swift sur Windows/Linux arrive en v2, sans build iOS.
- Arabe/RTL (v2). Thème clair/contraste élevé (v0.2).
- Palette glisser-déposer et édition complète des propriétés (v2).
- Debugger DAP, profiler, Compose Preview, refactoring maison, lint avancé (v2).
- Intégration Git complète (statut/clone éventuellement en v0.2).
- Multi-fenêtre et auto-update (v0.1.1).
- Télémétrie (opt-in, plus tard).
- Appareils physiques : bonus, premiers à couper.

### Liste de coupe

Si le retard s'installe, dans cet ordre :

1. Linux après Windows.
2. « Wrap with » de l'inspecteur (l'inspecteur reste pick → code).
3. Correction auto d'erreur de build par l'agent.
4. Thème clair.

Note : recommandation de l'architecte : inverser 2 et 3 (couper l'auto-fix avant « Wrap with »), car « Wrap with » est une seule code action LSP, peu coûteuse et au cœur de la valeur de l'inspecteur, tandis que l'auto-fix est une boucle build/agent plus risquée (parsing d'erreurs, retries, budget, permissions). La décision finale se prend au moment de la coupe. À ne jamais couper : sauvegarde/undo, sécurité, tests, packaging, docs de démarrage.

## 3. Principes

1. **Ne rien réécrire** : piloter LSP, SDK Android, Flutter, VM Service et scrcpy.
2. **Moteur sans UI** : l'interface est remplaçable ; le moteur ne dépend d'aucune UI.
3. **Tout est une commande** : menu, raccourci, CLI, agent IA et MCP passent par le même point d'entrée. Les commandes sont asynchrones, annulables, avec progression et handle de job.
4. **Headless d'abord** : chaque fonction existe en CLI avant l'UI.
5. **Zéro panic** : pas de `unwrap()`, `expect()`, `panic!` ni `unsafe` hors tests.
6. **Undo maîtrisé** : undo pour les documents et le projet ; les opérations externes (SDK, AVD, build, réseau) sont journalisées, pas annulées.
7. **Confiance par défaut** : workspace trust, token sur le canal de contrôle, niveaux de danger MCP, consentement avant envoi de code à l'API.
8. **Avancement honnête** : critères d'acceptation mesurables, pas de pourcentages décoratifs.

## 4. Roadmap (30-36 semaines)

| Phase | Semaines | Livrable |
|---|---|---|
| 0. Fondations + spikes | 1-3 | Workspace, CI, ADR 0001-0004, décision UI, prototypes mesurés |
| 1. Squelette headless + scrcpy | 4-8 | SDK/AVD/run/logcat + fenêtre scrcpy + UI minimale |
| 2. Éditeur + LSP + émulateur | 9-16 | Éditeur, Dart LSP, terminal, recherche, palette, settings, contrôles émulateur |
| 3. Inspecteur | 17-22 | Pick → fichier:ligne, Wrap with, propriétés read-only |
| 4. Agent + MCP | 23-26 | Diffs, auto-fix optionnel, MCP read-only |
| 5. Durcissement + release | 27-30 | MSI/AppImage/deb, docs, perf, sécurité |
| Buffer | 31-36 | Imprévus, polish, report de la liste de coupe |

**30 semaines planifiées + 6 semaines de buffer = 36 semaines.**

### Jalons de décision

- **Fin S3** : décision UI (ADR 0001), streaming (ADR 0002), édition de code (ADR 0003). Pas de prolongation.
- **Fin S8** : sur une VM Windows neuve, SDK installé, AVD créé et démarré, app Flutter lancée, logcat visible, fenêtre scrcpy ouverte.
- **Fin S16** : éditeur 5 000 lignes fluide, Dart LSP, terminal, contrôles émulateur.
- **Fin S22** : clic widget → fichier:ligne < 300 ms, « Wrap with » + hot reload + undo.
- **Fin S26** : agent avec diffs, MCP read-only.
- **Fin S30** : v0.1 publique.
- **S31-36** : buffer, pas de nouvelle fonctionnalité majeure.

## 5. Détail par phase

### Phase 0 — Fondations + spikes (S1-3)

- Créer le dépôt et le workspace Cargo.
- Écrire `README.md`, `AGENTS.md`, `CLAUDE.md`, `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`, `SECURITY.md`.
- Implémenter `cargo xtask ci` (fmt, clippy, tests, layers, build) et `cargo xtask layers`.
- `pls-base` : erreurs, tracing, chemins, settings atomiques, processus, bus d'événements, types de commandes.
- Trois spikes :
  - **Éditeur** : egui vs GPUI vs webview + Monaco/CodeMirror. Critères : IME français, 60 fps, RAM, coût en semaines, licence. Défaut egui si aucun ne se détache.
  - **Streaming** : gRPC `streamScreenshot` vs scrcpy H.264. Mesures fps, latence, CPU/GPU.
  - **Inspecteur** : VM Service, mode sélection Widget Inspector, clic via scrcpy, `file:line`, « Wrap with Padding ».
- Modèle de menace et brouillon du protocole de contrôle.

### Phase 1 — Squelette headless + scrcpy (S4-8)

- `sdk.detect` : SDK, JDK, Flutter, Node, Git, adb, espace disque, accélération.
- Téléchargement de `cmdline-tools` avec reprise, checksums et proxy.
- `sdk.install`, licences explicites, `sdkmanager`.
- `avd.list`, `avd.create`, `avd.delete`, `avd.start`, `avd.stop`, détection du boot.
- `run.start`, `run.stop`, `run.hotReload`, `run.hotRestart`, parsing `flutter run --machine`.
- `logcat.start`, `logcat.filter`, `logcat.clear`.
- `emulator.open` : lancement de scrcpy dans une fenêtre séparée ; `emulator.close` : fermeture de scrcpy ; `avd.stop` : arrêt de l'émulateur.
- `editor.open`, `editor.save` minimal, buffer rope.
- UI minimale : projet, onglets, éditeur, logcat, terminal, barre d'outils.
- Fakes et fixtures pour les parsers.

### Phase 2 — Éditeur + LSP + émulateur (S9-16)

- Coloration tree-sitter (Dart, YAML, JSON, Gradle).
- Undo/redo, multi-curseur, recherche/remplacement, formatage.
- Client LSP générique, Dart analysis server : diagnostics, completion, définition, hover, code actions, format.
- Terminal PTY multi-onglets, ANSI.
- Palette de commandes et menus générés depuis le registre.
- Settings (thème sombre, FR/EN, raccourcis), persistance atomique.
- Contrôles émulateur : rotation, capture, volume, retour/accueil/récents ; appareils physiques en bonus.
- Erreurs de build parsées et cliquables.
- Filtres logcat (niveau, tag, package, texte), pause, export.

### Phase 3 — Inspecteur (S17-22)

- Client VM Service (WebSocket JSON-RPC), connexion/déconnexion.
- Widget tree, sélection, `file:line` via `--track-widget-creation` (debug).
- Mode sélection du Widget Inspector : l'utilisateur clique dans la fenêtre scrcpy, l'app renvoie la sélection via le VM Service.
- Overlay de surbrillance dessiné par l'app, pas par l'IDE.
- Panneau Propriétés read-only, navigation vers le code.
- « Wrap with » via code action Dart LSP, diff de prévisualisation, hot reload, undo.
- États vides et erreurs.

### Phase 4 — Agent + MCP (S23-26)

- Provider trait + client Claude, streaming, retries, erreurs.
- Contexte : fichier ouvert, sélection, erreurs de build, logcat.
- `ToolHost` : lire, éditer, build, logcat ; permissions.
- Diff proposé/appliqué/refusé, détection de conflit, undo groupé.
- Boucle d'auto-fix bornée (optionnelle, coupe n°3), budget de tokens, annulation.
- Serveur MCP (`rmcp`), outils depuis le registre, danger levels, token, audit.

### Phase 5 — Durcissement + release (S27-30)

- Workspace trust, mode restreint, token du canal de contrôle, redaction des secrets.
- Tests de perf, soak test 8 h, bug bash.
- Packaging MSI, zip portable, AppImage, `.deb`, signature, checksums, SBOM.
- Docs : README, quickstart, troubleshooting, control protocol, MCP, privacy.
- Changelog, release GitHub, brouillon d'annonce.

### Buffer (S31-36)

- Imprévus, polish, accessibilité, documentation, reports de la liste de coupe.
- Aucune nouvelle fonctionnalité majeure.

## 6. Risques

| Risque | Probabilité | Parade |
|---|---|---|
| Éditeur maison trop long (IME, BiDi, a11y) | Élevée | Spike S1, option webview Monaco, défaut egui, décision fin S3 |
| Intégration scrcpy / latence | Moyenne | Fenêtre séparée en v0.1, flux intégré en v0.2 |
| Édition de code de l'inspecteur | Élevée | Pick + Wrap with seulement ; AST en v2 |
| Calendrier | Élevée | 30-36 semaines, buffer, liste de coupe |
| Onboarding JDK/Gradle/SDK | Moyenne | Assistant, détection JDK/Node/Git, reprise, proxy |
| Sécurité / workspace trust | Moyenne | Modèle de menace, mode restreint, tokens, consentement |
| Licences (Zed GPL, FFmpeg, H.264) | Moyenne | `cargo-deny`, scrcpy Apache-2.0, revue brevets |
| Nom `plstudio` | Moyenne | Handle GitHub pris ; dépôt personnel/org disponible ; vérifier domaine et marque |
| iOS Simulator sur Windows/Linux | Certaine | macOS uniquement ; remote Mac en v2.5 |
| OneDrive / chemins Windows | Moyenne | Sortir du dossier, chemins longs, exclusions Defender |

## 7. Qualité

- CI GitHub Actions : fmt, clippy, tests, layers, deny, build Windows et Linux.
- Tests unitaires par crate, intégration headless avec `plstudio-cli`, fakes pour SDK/AVD/adb/émulateur/LSP.
- E2E nightly sur runner Windows self-hosted avec émulateur.
- Snapshots UI selon l'UI choisie ; benchmarks rope/layout/démarrage ; fuzz logcat/NDJSON/protocole.
- Une tâche par commit, CI verte avant chaque commit.
- Docs à jour à chaque jalon ; ADR pour les décisions structurantes.
- Aucun `unwrap()`/`expect()`/`panic!`/`unsafe` dans le code de production (lints Clippy).

## 8. Communauté et publication

- Dépôt public dès la semaine 1, issues « good first issue ».
- Démos courtes (GIF/vidéo) à chaque jalon.
- Discussions GitHub ou Discord.
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, DCO.
- Annonce v0.1 : Reddit (r/rust, r/FlutterDev), Hacker News, X, LinkedIn.
- Vérifier le nom `plstudio` : handle GitHub déjà pris, domaines et marque à contrôler avant l'annonce.

## 9. Prochaines actions

1. Créer le dépôt `plstudio` et le workspace Cargo (hors OneDrive).
2. Écrire `AGENTS.md`, `README.md`, les licences et la CI.
3. Implémenter `xtask ci` et `xtask layers`.
4. Lancer les trois spikes (éditeur, streaming, inspecteur).
5. Décider l'UI à la fin de S3.
6. Démarrer le squelette headless : `sdk.detect` → `avd.create` → `avd.start` → `run.start` → `logcat`.
