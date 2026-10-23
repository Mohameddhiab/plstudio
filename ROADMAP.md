# Roadmap

La roadmap détaillée, les jalons, les risques et la liste de coupe sont dans **[plan.md](plan.md)**.

| Phase | Semaines | Livrable |
|---|---:|---|
| 0. Fondations + spikes | 1-3 | Workspace, CI, ADR, décision UI |
| 1. Squelette headless + scrcpy | 4-8 | SDK/AVD/run/logcat + fenêtre scrcpy |
| 2. Éditeur + LSP + émulateur | 9-16 | Éditeur, Dart LSP, terminal, contrôles émulateur |
| 3. Inspecteur | 17-22 | Pick → code, Wrap with |
| 4. Agent + MCP | 23-26 | Diffs, auto-fix optionnel, MCP read-only |
| 5. Durcissement + release | 27-30 | MSI/AppImage/deb, docs, perf, sécurité |
| Buffer | 31-36 | Imprévus, polish |

## Statut (23/10/2026)

- Phase 0 : **livrée** (workspace, CI, ADR, docs de gouvernance, branding).
- Phase 1 : **socle headless livré à fin HF-1** — moteur (jobs, validation, `engine.jobs`/`engine.cancel`), édition headless ([SPK-01](docs/spikes/SPK-01-edition.md)), CLI `run`/`commands`, **SDK/AVD et `pls-run` en 23/10 :**
  - `pls-sdk` : localisation des toolchains (env, défauts Windows, `cmdline-tools/latest`), `sdk.detect` sans processus.
  - `pls-avd` : `avdmanager` (list/create), `emulator -avd` détaché, `adb devices`, attente de boot annulable, `logcat -d` ; `spawn_detached` ajouté à `ProcessRunner`.
  - `pls-engine` : `sdk.detect`, `avd.list/create/start/devices/wait/logcat`, `emulator.scrcpy` (20 commandes au registre), état injectable pour tests.
  - `pls-run` : classification + parsing NDJSON de `flutter run --machine`, événements typés (`app.start/stop/debugPort/progress`, `daemon.exit`), `RunObserver` `Stopped→Starting→Building→Running→Stopping→Exited`.
- Phase 1 **restant** : streaming réel de `flutter run` (stdin `app.stop`, hot reload) avec le protocole de contrôle, fixtures E2E Windows plus tard.

v0.1 : Flutter/Android, Windows d'abord, Linux ensuite. React Native, Kotlin, Java, iOS et Swift sont en v2.
