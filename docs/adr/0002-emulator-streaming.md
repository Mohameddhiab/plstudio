# ADR 0002 — Streaming et contrôle de l'émulateur

- **Statut** : Accepté
- **Date** : 2026-10-21
- **Décideurs** : maintainer plstudio
- **Portée** : `pls-emulator`, `pls-inspector`, `pls-ui`, packaging, licences

## Contexte

L'émulateur Android peut être contrôlé de deux manières :

1. **gRPC `EmulatorController`** : `streamScreenshot`/`getScreenshot`, `sendMouse`, `sendKey`, rotation, statut. Authentification locale possible avec `-grpc-use-token`.
2. **scrcpy** : miroir H.264 basse latence, entrées souris/clavier, captures. Le serveur et le client sont Apache-2.0.

Le streaming intégré dans la fenêtre de l'IDE est coûteux : il faut un décodeur H.264 (matériel ou logiciel), gérer les licences/brevets, la latence, la mise à l'échelle et le rendu GPU. Les captures gRPC en PNG/JPEG sont plus simples mais peuvent être lourdes : une image 1080×2400 en RGBA brute pèse ~10 Mo ; à 30 fps, cela représente ~300 Mo/s à décoder et téléverser en texture.

La v0.1 doit d'abord valider le flux de travail complet (SDK → AVD → run → inspecteur) sans bloquer sur le décodeur vidéo.

## Décision

- **v0.1 : scrcpy dans une fenêtre séparée.** L'IDE contrôle le démarrage, l'arrêt, adb, la rotation, les captures, le volume et les boutons système. scrcpy affiche l'écran et transmet les entrées.
- **Pas de flux intégré en v0.1.** Le décodeur H.264 et le rendu dans le panneau sont reportés en v0.2, ou traités en bonus si le spike de streaming est très concluant.
- **gRPC en secours** pour les captures et certaines commandes si scrcpy ne suffit pas.
- **Inspecteur** : le clic vient de l'app Flutter elle-même. L'IDE active le mode sélection du Widget Inspector (`inspector.enableSelect`), l'utilisateur clique dans la fenêtre scrcpy, l'app renvoie la sélection via le VM Service. La surbrillance est dessinée par l'app.
- **Pas de hit-test sur une capture** en v0.1 ; le mapping de coordonnées reste nécessaire pour les captures et la rotation.

## Conséquences

- L'IDE ne peut pas dessiner d'overlay sur la fenêtre scrcpy : la surbrillance est dans l'app.
- L'expérience « émulateur intégré » est partielle en v0.1 ; le panneau de droite affiche les contrôles, l'état et les propriétés.
- Aucun décodeur H.264 à intégrer en v0.1 ; moins de risques de licences et de brevets.
- Le spike de streaming doit mesurer fps, latence, CPU/GPU, démarrage et entrées pour décider de la v0.2.
- L'inspecteur doit fonctionner aussi avec un appareil physique (même mode sélection via VM Service).

## Mesures du spike (S1-3)

| Métrique | Cible v0.1 | Cible v0.2 (flux intégré) |
|---|---|---|
| Première image après démarrage | < 2 s après boot | < 1 s |
| fps | 30 minimum | 60 |
| Latence entrée → affichage | < 100 ms | < 50 ms |
| CPU/GPU | Mesuré | < 20 % d'un cœur moderne |
| Rotation | Fonctionnelle | Fonctionnelle et fluide |
| Capture | PNG/JPEG | PNG/JPEG |

Si le spike montre que le flux intégré est simple et stable, il peut être avancé en bonus ; sinon, il reste en v0.2.

## Alternatives écartées

- **gRPC seul en v0.1** : plus de code de décodage, débit élevé, pas de gain UX immédiat.
- **Écrire un émulateur** : hors périmètre, coût déraisonnable.
- **Overlay IDE sur scrcpy** : impossible sans intégrer le flux ; reporté.
- **Wrapper scrcpy dans la fenêtre** : intégration native complexe, contraire à « ne rien réécrire ».

## Références

- `architecture.md` §7.3, §8.
- `plan.md` §5, phase 1 et phase 3.
- Spike streaming (S1-3), mesures à joindre.
