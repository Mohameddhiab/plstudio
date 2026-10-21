# Confidentialité

Statut : brouillon Phase 0.

## Principes

- **Aucune télémétrie par défaut.**
- Les logs restent locaux, sauf action explicite de l'utilisateur.
- L'agent IA n'envoie du code qu'après consentement explicite.
- Les clés API sont stockées dans le trousseau système ; jamais dans les logs.

## Données locales

| Donnée | Emplacement |
|---|---|
| Config | `%APPDATA%\plstudio` / `~/.config/plstudio` |
| Données | `%LOCALAPPDATA%\plstudio` / `~/.local/share/plstudio` |
| Logs | `.../plstudio/logs/plstudio.log` |
| Sessions | `.../plstudio/sessions/` |

Rotation des logs et purge des sessions anciennes à définir.

## Agent IA

- Le premier envoi demande un consentement clair : fournisseur, type de données, portée.
- `.plstudioignore` exclut des chemins.
- Redaction des motifs de secrets (clés, tokens, mots de passe) avant envoi.
- Budget de tokens et bouton d'annulation.
- Les diffs sont proposés, jamais appliqués sans validation.
- L'historique de conversation est local ; il peut être effacé.

## MCP et canal de contrôle

- Les appels sont auditrés localement.
- Les paramètres sensibles sont redacted.
- Le token de session est supprimé à la fermeture.

## Crash reports

- Opt-in uniquement, avec aperçu du contenu avant envoi.
- Pas de code source, pas de secrets, pas de chemins utilisateur par défaut.
