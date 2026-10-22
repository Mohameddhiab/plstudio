# Brief IA — correction et amélioration du branding plstudio

Objet : 8 rendus du système de marque actuel à critiquer puis améliorer.
Méthode d'envoi : joindre les 8 PNG ci-dessous + ce brief.

## Contexte global

- **Produit** : plstudio — IDE Rust pour Flutter/Android, sombre uniquement (v0.1).
- **Animal choisi** : le fennec (renard du désert) ; la marque relie le désert et le code.
- **Signature attendue** : tête de fennec flat + accolades de code `< >` ; mascotte chibi « Fen ».
- **Palette (tokens)** :
  - nuit `#0B0F14` — fonds
  - sable `#E7B36A` — corps du fennec, accent principal
  - cuivre `#C96A3D` — accolades, oreilles internes, « pl » du wordmark
  - crème `#F4E7D3` — museau, ventre, « studio », texte sur fond sombre
  - focus `#5EEAD4` — UI uniquement, jamais dans les logos
- **Typographie** : Inter (UI) / Manrope (display, wordmark).
- **Contraintes de style** : flat vectoriel, sans dégradé, sans ombre portée, sans
  brillance, lisible à petite taille ; drawable sur fond nuit.

## Les 8 images

### 01-logo-mark-512.png — marque principale (rendu sur nuit)

- **Source** : `logo-mark.svg` (512×512, fond transparent).
- **Contenu** : tête de fennec flat (sable), oreilles internes cuivre, museau
  crème, yeux ronds nuit avec reflet crème, accolades `< >` cuivre de chaque
  côté.
- **Ce qui marche** : silhouette continue, reflets des yeux, accolades bien
  séparées des oreilles.
- **Axes d'amélioration** :
  1. la tête garde une lecture « lapin/nounours » : oreilles encore longues et
     étroites pour un fennec (elles devraient être plus larges à la base, avec
     un duvet) ;
  2. le dôme crânien entre les deux oreilles reste un peu saillant ;
  3. aucune details typique de fennec : plumes de joues, marquage fourrure,
     patte... (flat, mais une seule forme d'accent) ;
  4. la seule signature « code » est l'accolade — proposer une variante où
     l'accolafe est plus intégrée (oreille qui forme un chevron ?) ;
  5. à 64px, le museau crème et la bouche deviennent presque invisibles.

### 02-logo-mark-64.png — même mark à 64 pixels

- **Ce qui marche** : lisible et reconnaissable en très petit.
- **Axes d'amélioration** : les détails fins (museau, bouche, reflet) se
  perdent ; prévoir une variante simplifiée spéciale < 24px (couper la bouche
  et le reflet d'œil).

### 03-logo-mark-mono-512.png — version une couleur

- **Source** : `logo-mark.mono.svg` — tout en `currentColor` (ici crème sur
  nuit, teinté via CSS).
- **Contenu** : silhouette complète en une couleur ; les creux (oreille
  interne, museau, nez, yeux) sont en négatif (transparence).
- **Ce qui marche** : le négatif est cohérent et lisible.
- **Axes d'amélioration** :
  1. le museau knockout est grand et à dominante « bulle » — sens de lecture
     un peu flou ;
  2. rims inégaux entre oreille externe et interne (10 à 28 px sur 512) ;
  3. tester une alternance où l'oreille interne knockout est remplacée par
     une découpe plus profonde (plus d'un seul seuil d'accent).

### 04-lockup-horizontal.png — cite horizontal (mark + wordmark)

- **Source** : `logo-lockup-horizontal.svg` (1144×512).
- **Contenu** : mark à gauche ; à droite « pl » en cuivre + « studio » en crème,
  Manrope 700 en fallback Segoe UI (le vrai Manrope n'est pas encore embarqué),
  largeur de mot épinglée par `textLength`.
- **Ce qui marche** : proportions mark/texte correctes, contraste des deux
  tones.
- **Axes d'amélioration** :
  1. la vraie typo Manrope n'est pas utilisé — à vectoriser (courbes) pour
     un rendu stable dans tous les logiciels ;
  2. l'accent cuivre/crème coupe le mot « brutalement » : tester séparateur
     discret (« pl.studio » ? accent graphique ?) ;
  3. l'embrasure mark↔texte est grande ; équilibrer avec le weight choisi ;
  4. mineurs : harmoniser la ligne de baseline du texte avec le centre
     optique du mark.

### 05-lockup-vertical.png — cite vertical

- **Source** : `logo-lockup-vertical.svg` (512×640).
- **Axes d'amélioration** : même question de wordmark ; vérifier l'air
  vertical entre mark et texte (~70px actuellement).

### 06-app-icon-512.png — icône d'application

- **Source** : `app-icon.svg` (512, fond nuit `rx=96`).
- **Ce qui marche** : contraste fort, fond nuit plein.
- **Axes d'amélioration** :
  1. le mark pourrait remplir davantage (le mark n'utilise que
     ~80% de la surface en hauteur) ;
  2. le rayon 96/512 sert d'iOS-like ; tester l'arrondi Android
     (≈ 22-23 %) et un masque circulaire pour le GitHub avatar ;
  3. aucune idée du "IDE" autre que l'accolade : tester un accent de focus
     (curseur block, barré de couleur « focus » interdit dans le logo...)

### 07-favicon-64.png — favicon professionnel (lisible dès 16px)

- **Source** : `favicon.svg` (64×64, fond nuit `rx=14`, frangin à 0.125
  du mark).
- **Ce qui marche** : silhouette tête+oreilles globale tient le choc à 16px.
- **Axes d'amélioration** :
  1. à 16px, les détails (museau, bouche, reflet) deviennent du bruit :
     prévoir une version simplifiée en ≤ 24px ;
  2. la tonalité du museau crème se confond avec le sable aux petites
     tailles — tester plus de contraste.

### 08-social-preview-1280x640.png — bannière sociale GitHub

- **Source** : png social exporté depuis `logo-lockup-horizontal.svg` en
  1280×640, fond nuit, lockup centré.
- **Ce qui marche** : simple, lisible dans le flux GitHub.
- **Axes d'amélioration** :
  1. tout le contenu repose sur le wordmark texte — mettre à jour avec une
     version vectorisée de la marque d'abord ;
  2. composition un peu vide : tester une composition avec décors discrets
     (dunes flat, terminal, grille) sans mieux étouffé les accents ;
  3. réserver de la place pour une éventuelle slogan court une fois v0.2.

## Ce qu'on attend du modèle d'IA

1. **Critique visuelle** de chaque image : tout ce qui gêne la lecture d'un
   fennec, d'une icône IDE, du système de marque général.
2. **Suggestions concrètes** (formes, proportions, couleurs) en respectant
   la palette et le flat vectoriel.
3. **Une proposition de directional alternative pour le mark** (si idée
   meilleure que l'accolade externe).
4. Ne pas proposer de dégradés, d'ombres, d'effets 3D, d'ajout d'animal
   secondaire, ou de réécriture du wordmark (police sortie en règles).
