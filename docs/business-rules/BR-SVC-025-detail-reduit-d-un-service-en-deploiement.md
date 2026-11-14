---
id: BR-SVC-025
domaine: SVC
titre: Le détail d'un service en déploiement ne porte pas la carte « Suppression »
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-025), contexts/hearth/conceptions/2026-10-13-design-deploiements-lots-suivants.md (écart 14.17), HRT-86
maj: 2026-11-14
---

# BR-SVC-025 : Le détail d'un service en déploiement ne porte pas la carte « Suppression »

## Règle
Le détail d'un service dans l'état « Déploiement en cours » est réduit : il ne porte aucun réglage, donc pas la carte « Suppression ». La raison « Un déploiement est en cours pour ce service. Annule-le d'abord. » (BR-DEL-003) ne se dit que sur la ligne du service, jamais dans le détail.

## Application (code)
- `apps/desktop/src/services/delete.ts::deleteDoor` (état `deploying` : porte `hidden`)
- `apps/desktop/src/components/molecules/DeleteServiceCard.vue` (aucune carte pour une porte `hidden`)

## Vérification
- `apps/desktop/src/services/delete.test.ts` (la porte du bouton)
- `apps/desktop/src/pages/ServicesDelete.test.ts` (un service en déploiement n'a pas la carte)

## Cas limites
- HRT-86 ne réduit que la carte « Suppression » : le reste du détail réduit (nom, état, étapes, « Annuler le déploiement ») n'est pas de ce ticket.

## Règles liées
- BR-DEL-003
- BR-DEL-026

## Historique
- 2026-11-14 : création (HRT-86, côté client).
