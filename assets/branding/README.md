# Branding plstudio

Identité visuelle de **plstudio** : un fennec (le renard du désert) encadré par des
accolades de code `< >`, plus une mascotte chibi, **Fen**.

## Palette

| Nom    | Hex       | Usage                                                          |
| ------ | --------- | -------------------------------------------------------------- |
| night  | `#0B0F14` | fonds sombres, carrés des icônes, surface UI primaire          |
| sand   | `#E7B36A` | corps et oreilles du fennec, accent principal sur fond sombre   |
| copper | `#C96A3D` | accolades du code, oreilles internes, « pl » du wordmark        |
| cream  | `#F4E7D3` | museau, ventre, queue du fennec ; « studio » ; texte sombre     |
| focus  | `#5EEAD4` | états succès / focus, UI uniquement — jamais dans les logos     |

Source de vérité : [`tokens.json`](tokens.json).

## Typographie

- **Inter** — interface.
- **Manrope** — display et wordmark (`plstudio`, Manrope 700).

Le wordmark des lockups reste un élément `<text>` (avec `textLength` épinglé pour
stabiliser la mise en page selon les moteurs). À convertir en contours
vectoriels avant la v0.2, le temps que les polices soient embarquées.

## Fichiers

| Fichier                        | Usage                                                       |
| ------------------------------ | ----------------------------------------------------------- |
| `logo-mark.svg`                | marque principale (fennec + accolades)                      |
| `logo-mark.mono.svg`           | version une couleur (`currentColor`, se teinte via CSS)     |
| `logo-lockup-horizontal.svg`   | en-têtes, pages web                                         |
| `logo-lockup-vertical.svg`     | écrans de chargement, tour de cou vertical                  |
| `app-icon.svg`                 | icône d'application 512, fond nuit, coins 96 px             |
| `favicon.svg`                  | favicon 64, lisible dès 16 px                               |
| `mascot-fen.svg`               | mascotte Fen, chibi assise avec la queue recourbée          |
| `tokens.json`                  | palette, typographie, formes                                |
| `png/`                         | exports PNG (app-icon 256→16, favicon 16/32, avatar GitHub, social preview) |

## Règles d'usage

- **Surfaces** : la version couleur est dessinée pour des fonds sombres
  (`#0B0F14` ou plus sombre). Sur fond clair, utiliser la version
  `logo-mark.mono.svg` teintée (ex. `color: #0B0F14`).
- **Tailles minimales** : le mark seul ≥ 24 px de hauteur ; en dessous,
  utiliser `favicon.svg`. Lockups : horizontal ≥ 96 px de hauteur,
  vertical ≥ 128 px.
- **Zone de protection** : garder autour de la marque une marge libre de
  12 % de la hauteur de son contenu (≈ 60 px sur un canvas 512). Pas
  d'élément graphique dans cette zone.
- **Interdits** : étirer, faire pivoter, ajouter des dégradés, des ombres
  portées ou un effet de brillance, recolorer les formes une à une.
- **Mono** : tout le contenu de `logo-mark.mono.svg` utilise
  `currentColor` ; fixer la couleur par CSS, sans jamais toucher au
  fichier.
- La mascotte Fen garde ses proportions chibi ; ne pas dissocier tête et
  corps.

## Regénérer les exports PNG

Les PNG de `png/` sont produits par Edge en mode headless : un petit
wrapper HTML (taille voulue, fond `transparent`) est incluant le SVG,
puis `--screenshot` avec `--window-size` et
`--default-background-color=00000000`. Les tailles actuelles :
app-icon 256/128/64/48/32/16, favicon 16/32, avatar GitHub 1024, social
preview 1280×640 (fond nuit, lockup horizontal centré).
