---
id: BR-RESIL-018
domaine: RESIL
titre: Une même notification répétée devient un compteur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-018), HRT-09
maj: 2026-10-05
---

# BR-RESIL-018 — Une même notification répétée devient un compteur

## Règle
Quand la même notification (même type, même texte) est émise plusieurs fois, la pile n'ajoute pas de ligne : un compteur « 5 fois » apparaît sur la ligne existante et sa durée d'affichage est renouvelée. Des textes ou des types différents restent séparés. La file ne dépasse jamais 50 notifications (session longue, BR-RESIL-017).

## Application (code)
- `apps/desktop/src/stores/toasts.ts::push` (déduplication par type et texte, `count`) ; affichage `apps/desktop/src/components/molecules/ToastStack.vue` (`toast__count`).

## Vérification
- Tests : `link-stores.test.ts::counts a repeated notification instead of stacking it`, `::does not merge the same text of another kind`, `::dismisses on its own after the lifetime, and a repeat renews it`, `molecules.test.ts::turns a repeated notification into a counter`, `errors.test.ts::bounds the rate`, `e2e/shell.spec.ts`.

## Cas limites
- Les notifications masquées par la limite de 3 gardent leur compteur.

## Règles liées
- BR-RESIL-011.

## Historique
- 2026-10-05 — création (HRT-09, session 2026-10-04-hearth-creation). Portée par l'interface ; le calcul des états est dans `hearth-link` (ADR-0007).
