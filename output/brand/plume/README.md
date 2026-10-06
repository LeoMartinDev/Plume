# Plume — kit de marque

Le P approuvé a été redessiné en SVG à partir du concept ImageGen, pour des contours nets et des déclinaisons cohérentes. La petite pointe en haut à droite est conservée. Les images finales sont des exports de ce tracé vectoriel, pas des générations indépendantes.

## Fichiers

- `mark-white.svg` / `.png` : P blanc sur fond transparent, pour les surfaces sombres.
- `mark-black.svg` / `.png` : P noir sur fond transparent, pour les surfaces claires.
- `icon-dark.svg` / `.png` : P blanc sur carré noir aux coins arrondis, avec contour transparent.
- `icon-light.svg` / `.png` : P noir sur carré blanc aux coins arrondis, avec contour transparent.
- `wordmark-white.svg` / `.png` : symbole + Plume, transparent, pour le site sombre.
- `wordmark-black.svg` / `.png` : symbole + Plume, transparent, pour le site clair.
- `icon-{16,32,48,64,128,180,192,256,512,1024}.png` : exports avec fond noir pour favicon, icône tactile, manifeste web et application.
- `favicon.ico` : tailles 16, 32, 48 et 256 intégrées.
- `plume.icns` : icône macOS avec variantes standard et Retina.
- `preview.png` : aperçu des six déclinaisons sur fonds adaptés.

Les symboles et icônes PNG maîtres font 1280 × 1280 pixels ; les signatures font 2560 × 800 pixels. Les SVG s'adaptent à toute taille. La signature SVG utilise Arial/Helvetica ; le PNG fige le rendu de la typographie.

L'icône d'application utilise les marges habituelles des icônes macOS : sur un canevas de 1024 pixels, le fond mesure 824 pixels, avec 100 pixels de marge transparente et un rayon de 185 pixels. Le tracé source est `crates/stt-app/assets/brand/plume-app.svg`. `scripts/generate-icons.mjs` régénère les PNG, les ICO Windows/favicon et les pixels de l'icône de fenêtre Linux à partir de ce SVG ; exécuté sur macOS, il régénère aussi les ICNS avec `iconutil`. Il nécessite Node.js et le module `sharp` (installé localement ou accessible via `NODE_PATH`).

## Usage

Préférer le SVG sur le site. Utiliser le P seul pour les icônes et les petites surfaces. Garder les marges intégrées et les proportions ; ne pas étirer le symbole. Noir #000000, blanc #FFFFFF.

Dans l'app, `crates/stt-app/assets/brand/plume.svg` est utilisé à côté du nom dans la barre latérale des réglages et teinté selon le thème. Les fichiers d'icône système sont prêts pour le packaging ; ce kit ne modifie pas à lui seul les icônes natives de la barre de menus.

## Origine

Concept approuvé : ImageGen intégré, P blanc sur fond noir. Prompt : « A compact abstract lowercase p monogram for Plume, built as one bold coherent shape: a vertical rectangular stem merging into a clean circular bowl, with a precisely cut circular black counter. Give the upper-right outer contour a subtle tapered geometric cut. »

Une extraction transparente via ImageGen a été essayée mais écartée pour ses contours imparfaits. Prompt : retirer uniquement le fond noir et le contreforme circulaire, préserver la silhouette blanche du P. Les déclinaisons livrées utilisent toutes le même tracé SVG redessiné.
