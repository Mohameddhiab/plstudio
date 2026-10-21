# ADR 0003 — Édition de code depuis l'inspecteur

- **Statut** : Accepté
- **Date** : 2026-10-21
- **Décideurs** : maintainer plstudio
- **Portée** : `pls-inspector`, `pls-lsp`, `pls-agent`, `pls-ui`

## Contexte

Le mode visuel doit permettre de cliquer sur un widget dans l'app pour :

1. le sélectionner ;
2. ouvrir le fichier à la bonne ligne ;
3. éventuellement le modifier (entourer de `Padding`, changer une couleur, etc.).

Le VM Service de Flutter fournit le widget tree, la sélection et, avec `--track-widget-creation` (build debug), la localisation de création `fichier:ligne`. Le Dart LSP fournit des code actions comme « Wrap with Padding ».

En revanche, **modifier arbitrairement une couleur, un padding ou insérer un widget par glisser-déposer n'est pas couvert par les code actions**. Cela demanderait un moteur d'édition AST (helper Dart avec `analyzer`/`dart_style`) ou des éditions textuelles fragiles. C'est un chantier à part.

## Décision

### v0.1

- **Pick → fichier:ligne** : oui.
- **« Wrap with »** : oui, via code action Dart LSP (Padding, Center, Column, Row).
- **Propriétés read-only** : oui (nom, type, taille, position).
- **Pas de glisser-déposer**, pas d'édition de couleur/taille/texte dans le panneau.
- **Pas d'édition textuelle heuristique** pour les propriétés.
- Toute édition passe par une code action LSP, avec diff de prévisualisation, `dart format` si nécessaire, puis hot reload.
- Les edits sont groupés en une seule étape d'undo.
- Le mode visuel nécessite un build **debug** avec `--track-widget-creation`.

### v2

- Moteur d'édition AST (helper Dart) pour les propriétés et l'insertion.
- Glisser-déposer depuis une palette de widgets.
- Prévisualisation et undo par transaction.
- Support React Native/Kotlin/Java si l'inspecteur est étendu.

## Conséquences

- La v0.1 est un inspecteur « lecture + wrap », pas un éditeur visuel complet.
- Le clic vient de l'app (mode sélection du Widget Inspector), pas d'un hit-test sur une capture. La sélection est renvoyée via le VM Service.
- La surbrillance est dessinée par l'app (en v0.1, scrcpy est une fenêtre séparée).
- Les code actions dépendent du Dart LSP et de la version de Flutter ; prévoir des tests de contrat.
- Les builds release/profile ne fournissent pas `--track-widget-creation` : l'inspecteur est limité au debug.
- Le « Wrap with » est peu coûteux et conserve la valeur du mode visuel ; l'auto-fix de build est un chantier distinct, plus risqué.

## Alternatives écartées

- **Éditions textuelles par regex** : casse le code, pas fiable.
- **Moteur AST dès la v0.1** : coût élevé, retarde l'inspecteur.
- **Glisser-déposer complet dès la v0.1** : dépend du mapping de position, de l'AST et du LSP ; v2.
- **Hit-test sur capture** : ne fonctionne pas de manière fiable sans accès au rendu de l'app ; le mode sélection de l'app est plus robuste.

## Références

- `architecture.md` §7.3, §7.4.
- `design.md` §3.6.
- Spike inspecteur (S1-3), mesures à joindre.
