---
id: BR-RES-037
domaine: RES
titre: Toute création porte une limite de mémoire et de processeur, « Sans limite » n'existe plus
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-paliers-de-memoire.md (BR-RES-037), contexts/hearth/conceptions/2026-10-13-design-deploiements-lots-suivants.md (18.1), HRT-146
maj: 2026-10-17
---

# BR-RES-037 — Toute création porte une limite de mémoire et de processeur, « Sans limite » n'existe plus

## Règle
Tout service créé par Hearth a une limite de mémoire et une limite de processeur. Le composant de réglage n'a ni case, ni position, ni saisie qui retire une limite. Une saisie vide ou refusée est signalée sur place et la valeur reste « absente » : le formulaire qui porte le composant ne peut pas partir. Quand les bornes de la machine ne sont pas lues (pas de réponse de `GET /engine/capacity`), rien ne se règle et rien ne part, et l'écran le dit. L'agent refuse aussi une création sans limites (`LIMITS_REQUIRED`, HRT-68) ; cette fiche couvre le client.

## Application (code)
- `apps/desktop/src/resources/ready.ts::resourcesIssue`
- `apps/desktop/src/components/organisms/ResourceSliders.vue` (aucune sortie « sans limite », valeur vide ou refusée à `null`)

## Vérification
- `apps/desktop/src/resources/resources.test.ts` (`resourcesIssue`)
- `apps/desktop/src/components/organisms/resourceSliders.test.ts` (aucune sortie « sans limite », valeur vide, bornes absentes)

## Cas limites
- Un service existant sans limite (`BR-RES-041`) relève de l'onglet « Ressources » (HRT-90), pas de ce composant.

## Règles liées
- BR-RES-004, BR-RES-039

## Historique
- 2026-10-17 — HRT-146 : côté client.
