# SPK-01 — Édition de code (socle headless)

- **Statut** : socle livré (23/10/2026)
- **Décision UI** : toujours ouverte ([ADR 0001](../adr/0001-ui.md)) — ce socle est volontairement indépendant des trois candidats (egui, GPUI, webview)
- **Liés** : [architecture.md](../../architecture.md), [ADR 0003](../adr/0003-code-editing.md)

## Objectif

Établir ce que n'importe quelle option d'UI exigera pour l'édition, sans écrire une UI :
buffers en mémoire, éditions triviales et positions, recherche, undo/redo groupé,
transactions — le tout exposé comme commandes moteur, utilisable depuis la CLI
headless (`plstudio-cli run`). L'UI consommera le même store, quelle qu'elle soit.

## Livré

### `pls-text` (couche 1)

- Rope (`ropey` 1.6), **indices de caractères partout** (insert/remove/slice sont
  indexés en chars dans ropey 1.6).
- `CharSpan` validé, `Caret` (édition appliquée + son inverse), `Position`
  ligne:colonne (col en caractères, terminateur exclu ; bornage sur le `\n` du
  CRLF ; positions de fin de document normalisées).
- `History` : pas d'undo **groupés par transaction**, redo tronqué à chaque
  nouvelle édition, pile bornée à 1024 pas (comme tout éditeur).
- `find` / `find_all` naïfs (suffisant Phase 1, à optimiser avec les segments
  ropey si la mesure l'exige), `replace_all` en **un seul pas d'undo**, sans
  chevauchement ni boucle sur textes qui s'auto-remplissent.
- `transaction` : groupe d'éditions = un pas d'undo ; échec = rollback complet
  (texte et flag dirty restaurés), transactions imbriquées fusionnées.

### `pls-engine` (couche 4)

- `EditorStore` persistant entre commandes : `BufferId` stables (DTO dans
  `pls-base`), réouverture d'un fichier déjà ouvert = réutilisation du buffer
  (la mémoire fait foi), fermeture refusée sur buffer sale sauf `discard: true`.
- Commandes `editor.*` : `open`, `get`, `edit`, `undo`, `redo`, `save`,
  `find`, `list`, `close` ; `editor.edit` marqué `undoable`, silencieux sur
  les éditions sans effet. `find` est borné par `limit` (défaut 200, max 1000).
- Validation des paramètres systématique (`parse_params`) : objet JSON attendu,
  erreurs structurées `PLS-E-1004` avec remédiation.
- Suivi des jobs : table `running/succeeded/failed/cancelled`, commandes
  `engine.jobs` (Read) et `engine.cancel` (Write), annulation coopérative
  vérifiée de bout en bout.

### `plstudio-cli`

- `run --cmd <id> [--param {json}] [--workspace dir] [--trust trusted|restricted]` :
  résultat JSON sur stdout, erreur `{code, message, remediation}` sur stderr.
- Une commande par processus en Phase 1 (pas de démon) ; le protocole de
  contrôle longue durée arrive ensuite.

## Décisions notées pour l'ADR 0001

1. Le modèle de texte vit **côté moteur** : les trois candidats UI consommeront
   le même store de buffers ; leur coût se limitera au rendu et à l'input.
2. L'undo est groupé par transaction (usage machine naturel pour l'agent), avec
   l'inverse calculé au moment de l'édition (`Caret`).
3. Conversions ligne:col ↔ char au socle ; la correspondance **UTF-16 exacte**
   (contrat LSP) vient avec `pls-lsp`.

## Reste pour clore le spike

- Prototypes GUI des 3 candidats : démarrage, RAM, latence de frappe sur un
  gros buffer (500k lignes) — mesure réelle en s1-s3, puis décision.
- Conversion UTF-16 (`pls-lsp`).
- Recherche optimisée (segments `ropey`) si la mesure le justifie.
- Réconciliation buffers ↔ disque : watcher + hash du fichier ouvert (Phase 2).
