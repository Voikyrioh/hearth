---
id: BR-FICHE-063
domaine: FICHE
titre: Le Catalogue range les fiches en deux sections, se cherche sans majuscules ni accents et se filtre avec des comptes
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-choisir-un-jeu-dans-le-catalogue.md (BR-FICHE-063), HRT-241
maj: 2026-11-16
---

# BR-FICHE-063 — Rangement, recherche et filtres du Catalogue

## Règle
Le Catalogue range les fiches en deux sections, « Prêts à installer » puis « Bientôt dans Hearth », chacune triée par nom. La recherche garde les fiches dont le nom contient le texte saisi, sans tenir compte des majuscules ni des accents (60 caractères au plus, espaces de bord ignorés, ni gardée ni envoyée en quittant la page ; Échap la vide). Les filtres « Tous les jeux », « Prêts à installer » et « Bientôt » portent le nombre de cartes visibles avec la recherche en cours ; un seul à la fois, « Tous les jeux » au départ. Le filtre « Bientôt » et sa section n'existent que s'il y a au moins une fiche qui attend dans le catalogue de l'agent. Une fiche « bientôt » est une fiche en cours de preuve non installable ; elle n'a ni bouton, ni clic.

## Application (code)
- `apps/desktop/src/catalogue/sections.ts::{sectionsOf, counts, hasSoon, normalize}`
- `apps/desktop/src/pages/Catalogue.vue`, `components/molecules/{CatalogToolbar,FicheCard}.vue`

## Vérification
- `apps/desktop/src/catalogue/sections.test.ts`
- `apps/desktop/src/pages/Catalogue.test.ts` (section « les sections, la recherche et les filtres »)
- `apps/desktop/src/components/molecules/FicheCard.test.ts`

## Cas limites
- Un filtre dont le compte vaut 0 reste choisissable et affiche « Aucun jeu dans cette liste pour l'instant. » ; une recherche sans résultat affiche « Aucun jeu ne correspond à « {texte} ». Essayez un autre nom. ».

## Règles liées
- BR-FICHE-003, BR-FICHE-033, BR-FICHE-061, BR-FICHE-064

## Historique
- 2026-11-16 — création (HRT-241).
