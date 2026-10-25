# ADR 0005 — Canal de contrôle v0.1 : TCP local + token (pipes nommées ensuite)

- **Statut** : Accepté
- **Date** : 2026-10-23
- **Décideurs** : maintainer plstudio
- **Portée** : `pls-control` (nouvelle crate, couche 5), `plstudio-cli` (`serve`), couche sécurité du canal

## Contexte

`docs/control-protocol.md` (Phase 0) spécifie `plstudio-control/1` : JSON-lines,
handshake tokenisé, dispatch limité au registre de commandes. Le transport
spécifié en premier choix est la pipe nommée Windows / socket Unix ACL, avec
**repli local explicite** : TCP `127.0.0.1` + token obligatoire.

Le prochain chantier consommateur est l'UI : elle aura besoin d'exécuter des
commandes et de streamer les événements de run en direct. Les pipes nommées
demandent un effort spécifique par OS (ACL, descripteurs de sécurité,
redirection UDS `$XDG_RUNTIME_DIR`), alors que la logique de session
(handshake, gating, audit, mapping d'erreurs) est identique dans tous les cas.

## Décision

- **Nouvelle crate `pls-control` (couche 5)** : la logique de session vit dans
  une crate testable, derrière un trait `LineTransport` (duplex à lignes).
  Elle ne dépend que de `pls-engine` (4) et `pls-base` (0) — conforme à la
  hiérarchie ; `pls-ui`, `pls-mcp` et le binaire n'hébergeront aucune logique.
- **Transport livré en v0.1 : TCP 127.0.0.1 + token de session** (le repli
  documenté), token 32 octets de CSPRNG (`getrandom`), comparaison en temps
  constant, fichier de token hors projet (`%LOCALAPPDATA%`/XDG data dir) avec
  mode 0600 sur Unix, jamais dans les logs ni le Debug. La boucle
  d'acceptation n'écoute jamais sur une interface non locale.
- **La logique de session indépendante du transport** : handshake (`hello`,
  version `PLS-E-4002`, token `PLS-E-4001`), barrière « pas de méthode avant
  handshake » (`PLS-E-4006`), gate de confiance (`restricted` refuse
  `write`/`destructive` au canal), ligne bornée (1 MiB, enveloppe `PLS-E-4004`),
  audit par requête (méthode, issue, durée — jamais les paramètres).
- **Codes d'erreur** : les codes `PLS-E-4001..4007` du document vivent dans
  `pls-base` et tranchent les incidents du canal ; les échecs de commande
  gardent leur code natif (`PLS-E-1xxx`) avec la chaîne canonique en
  `data.code`. `events.subscribe` répond `PLS-E-1005` tant que le bus
  d'événements n'existe pas.
- **CLI** : `plstudio-cli serve` héberge le canal (une session par connexion,
  plusieurs connexions possibles, arrêt Ctrl+C). Défaut : `restricted`.

## Conséquences

- L'UI et les clients E2E peuvent se brancher dès maintenant sur TCP local ;
  l'introduction des pipes nommées/UDS se fera **derrière `LineTransport`**,
  sans toucher à `server.rs`.
- Le token TCP est une cible potentielle de lecture par un process local de
  l'utilisateur — stance identique à tout outil local à token (Docker, LSP) ;
  les pipes reprendront la barrière ACL utilisateur en plus.
- `events.subscribe` est un écart documenté : le flux live des `RunEvent`
  vers l'UI demandera le bus d'événements de `pls-base`, puis une extension
  du protocole (numéro de version inchangé, méthode ajoutée).
- Rate limit par client : non livré en v0.1 ; la ligne bornée et le
  handshake servent déjà de garde-fou minimal.
- `getrandom` entre en dépendance workspace (MIT/Apache-2.0), pas de GPL.

## Alternatives écartées

- **Pipes nommées d'emblée** : bloque la progression de l'UI sur un détail
  OS, et empêche les tests TCP CI simples ; reporté, pas abandonné.
- **WebSocket/HTTP** : enterre le JSON-lines simple, ajoute un serveur HTTP
  complet et des décisions de CSP pour rien (canal local mono-usage).
- **La logique du canal dans `plstudio-cli`** : binaire = colle mince ; la
  logique non testable dans un binaire viole la gouvernance du dépôt.
- **Réutiliser `pls-mcp`** : MCP a son propre contrat (`docs/mcp.md`,
  danger levels, confirmations) ; deux fronts distincts, le même engine.

## Références

- `docs/control-protocol.md` — contrat de protocole.
- `architecture.md` §6 (canal de contrôle, MCP).
- `docs/adr/0001-ui.md` — la future UI consommatrice.
