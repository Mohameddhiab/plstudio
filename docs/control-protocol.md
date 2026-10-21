# Protocole de contrôle plstudio

Statut : brouillon Phase 0. Ce document sera versionné et testé avant la v0.1.

## Objectif

Permettre à `plstudio-cli`, aux tests E2E et aux agents MCP de piloter une instance de plstudio (ou un moteur headless) sans passer par l'UI.

Le canal expose **le même registre de commandes** que l'UI. Il ne donne pas un accès direct au système de fichiers, aux processus ou au réseau.

## Transport

- Windows : named pipe `\\.\pipe\plstudio-<session>` avec ACL utilisateur.
- Linux : socket Unix `$XDG_RUNTIME_DIR/plstudio/<session>.sock`.
- Repli local : TCP `127.0.0.1:<port>` avec token obligatoire.
- Le canal n'écoute jamais sur une interface non locale.

## Handshake

```json
{"jsonrpc":"2.0","id":1,"method":"hello","params":{"protocol":"plstudio-control/1","token":"<token>","client":"plstudio-cli/0.1.0"}}
```

Réponse :

```json
{"jsonrpc":"2.0","id":1,"result":{"protocol":"plstudio-control/1","server":"plstudio/0.1.0","workspace":"C:\\src\\app","trust":"restricted","commands":42}}
```

Le token est généré par session, stocké avec des permissions restrictives, et comparé en temps constant.

## Messages

- `engine.execute` : exécuter une commande.
- `engine.cancel` : annuler un job.
- `engine.jobs` : lister les jobs en cours.
- `events.subscribe` : s'abonner à des topics.
- Les notifications serveur utilisent `method: "event"` et `params: {seq, ts, topic, payload}`.

Exemple :

```json
{"jsonrpc":"2.0","id":2,"method":"engine.execute","params":{"command":"sdk.detect","params":{},"wait":true,"request_id":"req-1"}}
```

Réponse :

```json
{"jsonrpc":"2.0","id":2,"result":{"job_id":7,"status":"completed","value":{"java_home":null,"flutter":"C:\\flutter\\bin\\flutter.bat"}}}
```

## Règles

- Toute commande est validée contre son JSON Schema avant exécution.
- `danger: read` est autorisé par défaut ; `write` et `destructive` exigent une confirmation explicite du client ou une politique de confiance.
- Un workspace inconnu démarre en `restricted`.
- Chaque appel est audité : commande, paramètres (secrets redacted), job, résultat, durée.
- Rate limit par client.
- Pas de commande UI (`requires_ui`) en headless : elles retournent une erreur explicite.
- Le protocole est versionné ; une rupture incrémente `plstudio-control/2`.

## Codes d'erreur

- `PLS-E-4001` : token invalide.
- `PLS-E-4002` : version de protocole non supportée.
- `PLS-E-4003` : commande inconnue.
- `PLS-E-4004` : paramètres invalides.
- `PLS-E-4005` : commande nécessitant une UI.
- `PLS-E-4006` : permission refusée.
- `PLS-E-4007` : job annulé.
