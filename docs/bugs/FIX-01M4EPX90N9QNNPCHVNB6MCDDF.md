---
id: FIX-01M4EPX90N9QNNPCHVNB6MCDDF
titre: Ajout d'un serveur : les éléments n'étaient pas alignés pareil à gauche (anneau de couleur, halo de focus, chiffres des étapes) (S3)
date_découverte: 2026-10-08
date_correction: 2026-10-09
---

# FIX-01M4EPX90N9QNNPCHVNB6MCDDF : Ajout d'un serveur : les éléments n'étaient pas alignés pareil à gauche (anneau de couleur, halo de focus, chiffres des étapes) (S3)

## Symptôme
Voiky, sur la forge (WebView2) : « les éléments ne semblaient pas tous alignés pareil à gauche », surtout les chiffres des étapes.

## Reproduction
Mesures sur sa capture `5.png` (501 px) : rond de l'étape 1 à x = 12, titre 12, libellés 13, champs 12 ; première pastille de couleur : anneau de sélection à x = 8 (4 px à gauche de l'axe, la pastille elle-même à 12) ; halo de focus du champ « Nom du serveur » à x = 9 (3 px) ; glyphes des chiffres 0,5 px au-dessus du centre de leur rond. `e2e/hrt47.spec.ts` aux 5 tailles : un seul axe gauche à 1 px près (rond, titre, libellé, champ au focus, première pastille sélectionnée avec son anneau) ; centre du glyphe = centre du rond à 0,5 px ; mêmes bords sur les fenêtres de création de compte et de changement de mot de passe (déjà alignées). Rouge avant : anneau 4 px, halo 3 px, glyphe 0,9 px.

## Cause root
L'anneau de la pastille sélectionnée et le halo de focus se dessinaient HORS de la boîte alignée ; le centrage des chiffres suivait la boîte de la ligne, que les polices déplacent d'un demi-pixel.

## Impacté
Le client (fenêtre d'ajout d'un serveur ; les halos de tous les champs).

## Workaround
Aucun.

## Correction
Rangée de pastilles décalée de la largeur de l'anneau ; halo de focus dessiné DEDANS (`inset`, champs et listes) : un seul axe gauche, anneaux et halos compris ; chiffres des étapes centrés par leur hauteur de capitale (`text-box: trim-both cap alphabetic`, interligne 1, chiffres tabulaires). Décision de Claude, à confirmer par Voiky (halo intérieur dans toute l'application). `// FIX:01M4EPX90N9QNNPCHVNB6MCDDF`.

## Règles
- Aucune règle métier.

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-47 (S3) ; capture `contexts/hearth/art/smoke-2026-10-08/5.png`
- Code : `components/molecules/StepTrail.vue`, `ColorSwatches.vue`, `atoms/HInput.vue`, `HSelect.vue`, `molecules/MultiSelect.vue`
