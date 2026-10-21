# Contribuer à plstudio

Merci de vouloir contribuer. plstudio est en pré-alpha : le périmètre de la v0.1 est volontairement réduit.

## Avant de commencer

1. Lis `AGENTS.md`, `plan.md`, `architecture.md` et `design.md`.
2. Vérifie qu'une issue existe pour ton changement. Les décisions structurantes passent par un ADR dans `docs/adr/`.
3. Pour un bug, joins les étapes de reproduction, la version de Windows/Linux, la version de Flutter, et les logs pertinents (sans secrets).

## Développement

```bash
cargo xtask ci
cargo xtask layers
cargo run -p plstudio-cli -- commands
```

Règles :

- Une PR = un sujet.
- Pas de `unwrap()`/`expect()`/`panic!`/`unsafe` en production.
- Tests unitaires pour toute logique ; fixtures dans `pls-testkit`.
- Pas de nouvelle dépendance GPL/AGPL.
- Respecte les couches de `architecture.md`.
- Pas de fonctionnalité hors périmètre v0.1 sans ADR.

## Commits et DCO

Les contributions sont acceptées sous **Developer Certificate of Origin 1.1**. Ajoute une ligne `Signed-off-by: Prénom Nom <email>` à chaque commit :

```bash
git commit -s -m "feat(avd): détecter le boot de l'émulateur"
```

Pas de CLA.

## Pull requests

- Décris le problème, la solution et les tests.
- CI verte obligatoire.
- Un mainteneur peut demander des changements de périmètre ou de sécurité.

## Sécurité

Ne publie pas de vulnérabilité dans une issue publique. Voir `SECURITY.md`.
