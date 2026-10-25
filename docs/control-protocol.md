# Protocole de contrôle plstudio

Statut : **v0.1 implémenté du transport TCP** (`pls-control`, ADR 0005) — pipes nommées/UDS et bus d'événements suivent. Ce document reste le contrat de référence.

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

## Implémentation v0.1 (`pls-control`)

Livré (ADR 0005) :

- **Transport** : TCP `127.0.0.1` uniquement, token de session obligatoire
  (32 octets CSPRNG, hexadécimal, comparaison en temps constant). Le token
  vit dans un fichier hors projet (`%LOCALAPPDATA%\plstudio\control\…` / XDG
  data dir ; mode 0600 sur Unix) ; la sortie standard du serveur contient le
  chemin du fichier, jamais le token. Lancement : `plstudio-cli serve
  [--port n] [--workspace dir] [--trust trusted|restricted] [--token-file p]`.
- **Framing** : une ligne = un objet `{"jsonrpc":"2.0","id","method","params"}`.
  L'`id` est obligatoire (pas de notifications entrantes en v0.1) et la
  réponse l'échoïte tel quel. Ligne bornée à 1 MiB (`PLS-E-4004` au-delà).
- **Barrière de handshake** : seule la méthode `hello` passe avant le
  handshake réussi ; tout le reste répond `PLS-E-4006`.
- **Méthodes** : `hello`, `engine.execute` (`{command, params?}` synchrone ;
  `wait`/`request_id` acceptés et ignorés), `engine.cancel` (`{job_id}`),
  `engine.jobs`, `commands.list`. `events.subscribe` répond `PLS-E-1005`
  tant que le bus d'événements n'existe pas.
- **Gate de session** : `restricted` (défaut `serve`) refuse les commandes
  `write`/`destructive` au canal (`PLS-E-4006`) ; `trusted` laisse passer
  tout le registre.
- **Audit** : chaque requête journalise la méthode, l'issue et la durée
  (`target: "control"`) — jamais les paramètres : on ne trace rien plutôt
  qu'à moitié redacted.

Cartographie des codes : les codes `PLS-E-4xxx` tranchent les incidents du
**canal** (token, version, méthode/commande inconnue via `PLS-E-4003`,
enveloppe, permission). Un échec de la **commande elle-même** garde son code
natif `PLS-E-1xxx` dans la réponse JSON-RPC (avec la chaîne canonique en
`data.code`) : un `PLS-E-1003` en réponse d'`engine.execute` peut donc
signifier « session de run inconnue », pas « canal cassé ».

Encore à venir : pipes nommées Windows / UDS Linux (même trait
`LineTransport`), bus d'événements + `events.subscribe`/notifications
serveur `event`, rate limit par client, ACL de pipe, montage inverse
(`--control` côté client).
