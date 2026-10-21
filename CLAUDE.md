# CLAUDE.md

Les instructions de ce dépôt sont dans **[AGENTS.md](AGENTS.md)**. Lis-le avant toute modification.

Résumé :

- plstudio, IDE Rust Flutter/Android ; v0.1 Windows d'abord, Linux ensuite.
- Couches strictes : `pls-base` → `pls-text`/`pls-sdk` → `pls-avd`/`pls-project` → `pls-lsp`/`pls-run`/`pls-emulator`/`pls-inspector`/`pls-agent` → `pls-engine` → `pls-mcp`/`pls-ui` → apps.
- Tout passe par des commandes async ; pas de logique métier dans l'UI.
- Pas de `unwrap()`/`expect()`/`panic!`/`unsafe` en production.
- v0.1 : Flutter uniquement, scrcpy séparé, inspecteur pick + Wrap with, thème sombre.
- `cargo xtask ci` doit passer.
