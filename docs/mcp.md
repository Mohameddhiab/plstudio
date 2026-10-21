# MCP — Model Context Protocol

Statut : brouillon Phase 0. Le serveur arrive en Phase 4.

## Lancement

```bash
plstudio-cli mcp
plstudio-cli mcp --attach <session>
```

- Sans `--attach`, le serveur crée un moteur headless.
- Avec `--attach`, il se connecte au canal de contrôle d'une instance plstudio ouverte.

## Outils exposés

| Outil | Danger | Description |
|---|---|---|
| `plstudio.commands` | read | Liste les commandes, schémas et niveaux de danger |
| `plstudio.execute` | dépend de la commande | Exécute une commande du registre |
| `plstudio.jobs` | read | Liste les jobs et leur progression |
| `plstudio.cancel` | write | Annule un job |
| `plstudio.inspect_project` | read | Détecte Flutter, SDK, AVD, appareils |
| `plstudio.screenshot` | read | Capture l'écran de l'IDE (pas scrcpy) |
| `plstudio.ui_click` | write | Clique un widget de l'IDE par id stable |

`plstudio.ui_click` ne cible pas la fenêtre scrcpy : l'inspecteur utilise le mode sélection de l'app Flutter.

## Sécurité

- Read-only par défaut.
- `write`/`destructive` exigent `--allow-write` ou une confirmation interactive.
- Token de session obligatoire en mode `--attach`.
- Audit de chaque appel.
- Rate limit.
- Aucun secret dans les descriptions ou les erreurs.

## Ce qui n'est pas exposé en v0.1

- Édition arbitraire de fichiers hors commandes.
- Exécution de commandes shell arbitraires.
- Accès réseau direct.
- Contrôle de l'émulateur via scrcpy.
