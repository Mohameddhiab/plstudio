# ADR 0004 — Horizon iOS et Swift

- **Statut** : Accepté (horizon v2)
- **Date** : 2026-10-21
- **Décideurs** : maintainer plstudio
- **Portée** : roadmap, `pls-project`, `pls-run`, `pls-lsp`, packaging macOS

## Contexte

La demande inclut, à terme, un « émulateur iOS » et le support de Swift sur Windows et Linux.

Contraintes :

- L'**iOS Simulator** fait partie de Xcode/CoreSimulator et ne fonctionne que sur macOS.
- Apple exige macOS pour builder, signer et exécuter une app iOS ; virtualiser macOS sur du matériel non-Apple viole l'EULA.
- La toolchain **Swift** officielle fonctionne sur Windows et Linux (SourceKit-LSP, SwiftPM, `swift-format`, LLDB), mais elle ne fournit pas le SDK iOS.
- Un « émulateur iOS » au sens d'un émulateur Android n'existe pas : c'est un simulateur Apple, lié à macOS.

## Décision

### v1 (v0.1)

- Android + Flutter uniquement. Aucune promesse iOS/Swift dans la v0.1.

### v2

- **macOS** : version macOS de plstudio avec iOS Simulator via Xcode/`xcrun simctl`, build/signature/déploiement iOS, et Swift via la toolchain Xcode.
- **Windows/Linux** : support du **langage Swift** via SourceKit-LSP, SwiftPM, `swift-format` et LLDB. Édition, compilation et tests de cibles serveur/CLI. **Pas de build iOS.**
- L'architecture de l'IDE reste la même ; `pls-project` détecte SwiftPM/Xcode, `pls-run` délègue aux outils Apple sur macOS.

### v2.5

- **Build iOS à distance** pour les utilisateurs Windows/Linux : un hôte macOS (Mac physique, CI ou cloud) exécute build/signature/simulateur, l'IDE s'y connecte. Périmètre et coût à définir.

### Interdits

- iOS Simulator sur Windows/Linux.
- Virtualisation de macOS sur du matériel non-Apple.
- Réutilisation de l'émulateur Android pour iOS.
- Redistribution du SDK iOS ou de Xcode.

## Conséquences

- La roadmap ne doit jamais annoncer « émulateur iOS sur Windows/Linux ».
- Le terme correct est **iOS Simulator**, pas « émulateur iOS ».
- Le support Swift sur Windows/Linux est un projet distinct du support iOS ; il peut être livré avant.
- Le remote Mac ajoute de la latence, de la sécurité (SSH/token) et de la maintenance.
- Les tests iOS nécessitent un runner macOS ; à prévoir dans la CI v2.

## Alternatives écartées

- **Cross-compilation Swift vers iOS depuis Windows/Linux** : ne remplace pas Xcode pour la signature et le simulateur.
- **Émulateur iOS open source** : inexistant de façon fiable et légale.
- **macOS virtualisé** : contraire à l'EULA Apple.
- **Attendre macOS pour tout** : retarde inutilement le support du langage Swift.

## Références

- `plan.md` §1, §2, §6.
- `architecture.md` §13.
- `design.md` §12.
