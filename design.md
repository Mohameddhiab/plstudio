# plstudio — Design (UI/UX)

Voir aussi : [plan.md](plan.md), [architecture.md](architecture.md), [docs/adr/](docs/adr/).

## 1. Principes

- **Rapide et léger** : démarrage < 2 s, 60 fps, aucune latence perceptible à la frappe.
- **Familier** : disposition proche d'Android Studio, pour une prise en main immédiate.
- **L'émulateur au centre du flux** : contrôlé par l'IDE ; en v0.1, scrcpy s'ouvre dans sa propre fenêtre. Le flux intégré arrive en v0.2.
- **Tout est commande** : menus, palette et raccourcis viennent du même registre.
- **Accessible** : clavier complet, contrastes AA, mouvement réduit, libellés sur les icônes.
- **Multilingue** : français et anglais en v0.1 ; arabe et RTL en v2.
- **Sombre par défaut** : thème clair et contraste élevé en v0.2.

## 2. Disposition de la fenêtre

```
┌──────────────────────────────────────────────────────────────────────┐
│ Menu │ Projet ▾ │ Appareil ▾ │ ▶ Run  ⟳ Hot reload  ■ Stop │ 🔍 Cmd │
├────────┬─────────────────────────────────────────┬───────────────────┤
│ Projet │ main.dart ✕ │ home.dart ✕ │ +            │ Appareil          │
│        │─────────────────────────────────────────│  Pixel 7 · API 34 │
│ lib/   │  1  import 'package:flutter/...';       │  ▶ Lancer         │
│  main  │  2                                      │  🖥 Ouvrir scrcpy │
│  home  │  3  class Home extends StatelessWidget  │  ⟲ ⤢ 📷 ⏻ 🖱      │
│ test/  │  ...                                    │                   │
│ pubspec│                                         ├───────────────────┤
│        │                                         │ Propriétés        │
│        │                                         │ Widget: Text      │
│        │                                         │ Couleur ▪ Taille  │
├────────┴─────────────────────────────────────────┴───────────────────┤
│ Logcat │ Terminal │ Problèmes │ Agent IA                              │
│ 12:01 I/flutter: Home built                                          │
├──────────────────────────────────────────────────────────────────────┤
│ Flutter 3.x │ Dart LSP ● │ Pixel 7 ● │ Ln 3, Col 12 │ UTF-8 │ FR     │
└──────────────────────────────────────────────────────────────────────┘
```

En v0.1, l'écran de l'émulateur n'est **pas** dessiné dans le panneau de droite : scrcpy s'ouvre dans une fenêtre séparée. Le panneau de droite affiche l'appareil, les contrôles, l'état et les propriétés du widget sélectionné. Le flux intégré est prévu en v0.2.

### Zones

| Zone | Contenu |
|---|---|
| Barre d'outils | Projet, appareil, Run, Hot reload, Stop, palette de commandes |
| Gauche | Explorateur de fichiers, recherche, structure |
| Centre | Éditeur à onglets, vue scindée possible |
| Droite | Appareil, contrôles émulateur, propriétés du widget, inspecteur |
| Bas | Logcat, terminal, problèmes, agent IA |
| Barre d'état | Framework, LSP, appareil, position, encodage, langue |

**Panneaux dockables** : regrouper, scinder, flotter dans la fenêtre, fermer, rouvrir. L'éditeur central ne peut pas être fermé. « Réinitialiser la disposition » restaure la valeur par défaut. La disposition est sauvegardée atomiquement.

## 3. Écrans principaux

### 3.1 Accueil

- Projets récents, « Ouvrir », « Nouveau projet ».
- État de l'environnement : SDK Android, JDK, Flutter, Node, Git, accélération matérielle, espace disque.
- Chaque ligne a un état ✓/✗ et un bouton « Corriger » avec une action concrète : installer Temurin, accepter les licences, activer Hyper-V, définir `JAVA_HOME`.
- « Cloner depuis Git » est optionnel en v0.1 ; sinon « Ouvrir un dossier ».

### 3.2 Nouveau projet

Étapes : **Type** → **Template** → **Nom, package, dossier** → **Créer**.

- v0.1 : Flutter uniquement.
- React Native, Kotlin et Java sont affichés grisés avec la mention « v2 ».
- Les templates Flutter délèguent à `flutter create` (vide, navigation, liste).

### 3.3 Assistant SDK

1. Détection : SDK, JDK, Flutter, Node, Git, disque, accélération.
2. Téléchargement de `cmdline-tools` (progression, reprise, checksums).
3. Licences : case « J'accepte », jamais automatique.
4. JDK : `JAVA_HOME` → `PATH` → JDK d'Android Studio → proposition d'installer Eclipse Temurin dans un dossier utilisateur, avec consentement.
5. Terminé.

Pas de JRE bundlé. JDK 17 minimum ; 21 recommandé selon la matrice Flutter/AGP/Gradle.

### 3.4 Device Manager

- Liste des AVD : nom, modèle, API, état, taille.
- Boutons : ▶ Lancer, ■ Arrêter, ✎ Modifier, 🗑 Supprimer.
- **Créer un appareil** : modèle (Pixel, tablettes, pliables, personnalisé avec largeur/hauteur/DPI/RAM) → version Android → type d'image (Google APIs, Play Store, AOSP) → architecture.
- Détection de l'accélération : WHPX/Hyper-V (Windows), KVM (Linux), avec message de correction.
- Appareils physiques : bonus v0.1, via adb.

### 3.5 Panneau émulateur

- L'IDE contrôle : démarrage, arrêt, adb, rotation, capture, volume, retour/accueil/récents, extinction.
- **Ouvrir scrcpy** lance la fenêtre séparée. L'IDE garde le focus sur les commandes et l'inspecteur.
- État « démarrage » avec indicateur ; temps de boot variable (30-120 s).
- Capture d'écran et rotation disponibles depuis l'IDE.
- v0.2 : flux intégré dans le panneau, avec décodage H.264.

### 3.6 Mode visuel (inspecteur)

- Bouton **🖱 Sélectionner** : active le mode sélection du Widget Inspector dans l'app.
- L'utilisateur clique dans la fenêtre scrcpy ; l'app sélectionne le widget et renvoie la sélection via le VM Service.
- La surbrillance est dessinée par l'app (Widget Inspector), pas par l'IDE.
- L'IDE affiche le widget, `fichier:ligne` et les propriétés en lecture seule.
- **« Wrap with »** : Padding, Center, Column, Row. L'action passe par une code action Dart LSP, affiche un diff, applique, puis déclenche le hot reload. Ctrl+Z annule.
- v0.1 : pas de glisser-déposer, pas d'édition de couleur/taille/texte dans le panneau. Ces fonctions arrivent en v2 avec un moteur d'édition AST.
- Le mode visuel nécessite un build debug avec `--track-widget-creation`.

### 3.7 Logcat

- Filtres : niveau (V/D/I/W/E), tag, package, texte (regex), appareil.
- Couleurs par niveau, pause/reprise, effacer, export.
- Erreurs et traces cliquables vers le fichier et la ligne.

### 3.8 Agent IA

- Chat avec contexte automatique (fichier ouvert, sélection, erreurs de build, logcat).
- Les modifications proposées s'affichent en **diff** (avant/après) avec Accepter / Refuser par fichier.
- Bouton « Corriger l'erreur » sur une erreur de build (optionnel, première fonction à couper).
- Indicateur de l'outil en cours (lecture, édition, build) ; bouton Stop.
- Consentement explicite avant l'envoi de code à l'API ; `.plstudioignore` et redaction des secrets.

### 3.9 Terminal et problèmes

- Terminal intégré multi-onglets.
- Panneau Problèmes : diagnostics du LSP groupés par fichier, avec correctifs rapides.

## 4. Palette de commandes (Ctrl+Shift+P)

Recherche floue dans **toutes** les commandes (`avd.start` apparaît comme « Appareil : lancer »). Affiche le raccourci associé. C'est aussi la porte d'entrée des agents.

## 5. Raccourcis (par défaut)

| Action | Raccourci |
|---|---|
| Palette de commandes | Ctrl+Shift+P |
| Ouvrir un fichier | Ctrl+P |
| Chercher partout | Ctrl+Shift+F |
| Enregistrer | Ctrl+S |
| Enregistrer sous | Ctrl+Shift+S |
| Run | Shift+F10 |
| Hot reload | Ctrl+S (enregistre puis recharge, avec debounce) |
| Hot restart | Shift+F5 |
| Stop | Ctrl+F2 |
| Aller à la définition | F12 |
| Correctif rapide | Alt+Entrée |
| Sélecteur visuel | Ctrl+Shift+I |
| Basculer le panneau émulateur | Ctrl+Alt+E |
| Agent IA | Ctrl+I |
| Terminal | Ctrl+` |

Ctrl+S enregistre **puis** déclenche le hot reload avec un debounce (300-500 ms). Le hot reload manuel reste disponible via la barre d'outils. Raccourcis modifiables ; schémas « Android Studio » et « VS Code » en v0.2.

## 6. Identité visuelle

### Thèmes

- v0.1 : sombre uniquement.
- v0.2 : clair, contraste élevé, et suivi du thème système en option.

### Palette (sombre)

| Rôle | Valeur |
|---|---|
| Fond principal | `#14161A` |
| Panneaux | `#1B1E23` |
| Bordures | `#2A2E35` |
| Texte | `#E6E8EB` |
| Texte atténué | `#8B93A0` |
| Accent (action principale) | `#3DDC97` |
| Info | `#4DA3FF` |
| Avertissement | `#F5B546` |
| Erreur | `#F25C5C` |

Contraste texte/fond ≥ 4,5:1 (WCAG AA). Les états ne reposent pas uniquement sur la couleur : icônes et libellés en complément.

### Typographie

- **UI** : Inter (SIL OFL).
- **Code** : JetBrains Mono (SIL OFL), ligatures en option.
- **Arabe** : Noto Sans Arabic (SIL OFL), v2.
- Tailles : UI 13 px, code 14 px, réglables.

### Iconographie

Icônes originales (SVG monochromes, 16/20 px). Pas d'icônes Google, JetBrains ou Microsoft. Chaque asset est listé dans `assets/ATTRIBUTION.md`.

### Marque

Le nom **plstudio** et son logo sont créés pour le projet ; aucune ressemblance avec les marques d'Android Studio ou de Google. Vérifier la disponibilité du nom, du domaine et de la marque avant l'annonce.

## 7. Comportements et états

- **États vides** : message court + action (« Aucun appareil — Créer un appareil »).
- **Chargement** : indicateur discret, jamais bloquant ; téléchargements avec progression et annulation.
- **Erreurs** : message clair, cause probable, action de correction (« Installer Temurin », « Activer Hyper-V », « Accepter les licences »).
- **Animations** : brèves (< 150 ms), désactivées si l'option « réduire les animations » du système est active.
- **Rendu** : le texte reste net à 100 %, 125 %, 150 %, 200 % de mise à l'échelle. La fenêtre scrcpy reste nette à son échelle native.
- **Fenêtre étroite** : sous 1 280 px, les panneaux droite et bas se replient en onglets ; l'éditeur reste prioritaire.

## 8. Internationalisation et RTL

- v0.1 : français et anglais ; changement à chaud, mémorisé.
- v2 : arabe et RTL natif (interface miroir, panneaux, icônes directionnelles, texte bidirectionnel dans l'éditeur).
- Aucune chaîne en dur : fichiers `locales/*.ftl` (Fluent).
- Pseudo-localisation et test des chaînes longues en CI.

## 9. Accessibilité

- Navigation complète au clavier, focus visible.
- Libellés accessibles sur toutes les icônes-boutons.
- Réduction du mouvement, contraste élevé (v0.2), taille de police réglable.
- Support lecteur d'écran : objectif best effort en v0.1, validation NVDA (Windows) et Orca (Linux) ; certification en v0.2 selon l'UI choisie.
- Option « curseur système visible » dans l'éditeur et le panneau émulateur.

## 10. Adressabilité pour les agents

- Chaque widget de l'IDE a un id stable (`button:run`, `panel:logcat`, `slider:…`) pour `ui.clickWidget`, `ui.widgets` et `ui.screenshot`.
- La fenêtre scrcpy est externe : `ui.clickWidget` ne la cible pas. L'inspecteur utilise `inspector.enableSelect` puis la sélection renvoyée par l'app.
- Les commandes MCP read-only sont disponibles par défaut ; les commandes write/destructive exigent une confirmation.

## 11. Critères de réussite UX

- Premier lancement → émulateur démarré et app Flutter exécutée en moins de 10 minutes (hors téléchargements).
- Frappe → affichage < 16 ms.
- Hot reload → écran mis à jour < 1 s.
- Sélection d'un widget par clic → saut vers le code < 300 ms.
- Ouverture de scrcpy → première image < 2 s après le démarrage de l'émulateur.
- Aucune action bloquante pendant un téléchargement ou un build.

## 12. Horizon iOS / Swift

- v0.1 : Android/Flutter uniquement.
- v2 : macOS, iOS Simulator via Xcode/`simctl`, Swift via Xcode.
- Swift sur Windows/Linux : SourceKit-LSP, SwiftPM, pas de build iOS.
- Windows/Linux → iOS : hôte macOS distant en v2.5.
- Pas d'iOS Simulator sur Windows/Linux ; pas de virtualisation macOS hors matériel Apple.
