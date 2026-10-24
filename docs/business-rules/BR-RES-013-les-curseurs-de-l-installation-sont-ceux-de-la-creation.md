---
id: BR-RES-013
domaine: RES
titre: Les curseurs de l'installation d'une fiche sont ceux de la création d'un service, placés sur ce que la fiche recommande
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-020), HRT-102
maj: 2026-10-24
---

# BR-RES-013

## Règle
Règle portée par le client (HRT-102). L'étape « Ressources » d'une installation réutilise `ResourceSliders` (HRT-146) : mêmes bornes, jamais « sans limite » (BR-RES-037). La mémoire part de la valeur recommandée PROUVÉE de la fiche (ramenée au maximum de la machine, ce que la page dit), le processeur de sa valeur recommandée prouvée, sinon de tous les processeurs du serveur avec « Hearth ne sait pas combien de processeur ce jeu demande. ».

## Application (code)
- `apps/desktop/src/fiche/fit.ts::placement`
- `apps/desktop/src/components/organisms/InstallResources.vue`
- `apps/desktop/src/components/organisms/ResourceSliders.vue`

## Vérification
- `apps/desktop/src/fiche/fiche.test.ts::place les curseurs sur le recommandé prouvé, sinon sur la proposition prudente et tous les processeurs`
- `apps/desktop/src/pages/InstallFromFiche.test.ts::l'étape « Ressources » (BR-FICHE-020 à 023, BR-RES-037)`

## Cas limites
- Les crans nommés sous le curseur (`MemoryTiers`) arrivent avec HRT-122 et HRT-123.

## Règles liées
BR-RES-004, BR-RES-037, BR-FICHE-020

## Historique
- 2026-10-24 — création (HRT-102).
