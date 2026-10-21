# Sécurité

plstudio est en pré-alpha. Aucune version n'est encore supportée en production.

## Signaler une vulnérabilité

- Utilise **GitHub Private Vulnerability Reporting** quand le dépôt sera public.
- En attendant, contacte le mainteneur en privé (GitHub) sans ouvrir d'issue publique.
- N'inclus pas de secrets réels, de clés API ou de données personnelles dans un rapport.

Nous visons une réponse sous 7 jours ouvrés et une divulgation coordonnée après correctif.

## Périmètre

Sont particulièrement sensibles :

- le canal de contrôle local (`plstudio-cli --control`, MCP) et son token ;
- l'exécution de commandes Gradle/Flutter/adb et les processus enfants ;
- le téléchargement du SDK Android et les checksums ;
- l'envoi de code à un fournisseur d'IA ;
- le stockage des clés API et des secrets ;
- l'ouverture d'un dépôt inconnu (workspace trust).

## Bonnes pratiques pour les contributeurs

- Pas de secrets dans les logs, les tests ou les fixtures.
- Pas d'exécution de commande construite par concaténation non échappée.
- Pas de `unsafe`.
- Toute nouvelle commande déclare son `danger` et sa politique de confirmation.
- MCP est read-only par défaut ; `write`/`destructive` exigent une confirmation.
