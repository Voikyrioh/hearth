---
id: BR-DEL-026
domaine: DEL
titre: « Supprimer ce service » n'est que dans le détail, jamais dans le menu de la ligne
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-026), HRT-86
maj: 2026-11-14
---

# BR-DEL-026 : « Supprimer ce service » n'est que dans le détail, jamais dans le menu de la ligne

## Règle
Une suppression ne porte que sur un service à la fois. Le bouton « Supprimer ce service » est dans la carte « Suppression » du bas du détail, séparé des actions de pilotage ; il n'existe pas dans le menu d'actions de la ligne. Un service en déploiement (détail réduit) n'a pas la carte ; un service adopté n'a qu'une phrase.

## Application (code)
- `apps/desktop/src/components/molecules/DeleteServiceCard.vue`
- `apps/desktop/src/services/delete.ts::deleteDoor`
- `apps/desktop/src/components/organisms/ServiceDetail.vue` (la carte)
- `apps/desktop/src/components/organisms/ServiceRowMenu.vue` (aucune entrée « Supprimer »)

## Vérification
- `apps/desktop/src/pages/ServicesDelete.test.ts` (la carte « Suppression »)

## Cas limites
- Aucun.

## Règles liées
- BR-DEL-002
- BR-DEL-003

## Historique
- 2026-11-14 : création (HRT-86, côté client).
