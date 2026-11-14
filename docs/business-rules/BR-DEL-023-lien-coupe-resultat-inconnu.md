---
id: BR-DEL-023
domaine: DEL
titre: Un lien coupé après l'envoi donne un résultat inconnu, jamais rejoué
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-023), HRT-86
maj: 2026-11-14
---

# BR-DEL-023 : Un lien coupé après l'envoi donne un résultat inconnu, jamais rejoué

## Règle
Si le lien coupe après l'envoi de la suppression et avant la réponse, la fenêtre de suivi dit « Le résultat de cette suppression n'est pas connu. » et que rien n'est relancé ; l'issue est annoncée au retour du lien par le suivi des opérations (BR-RESIL-010). L'acte n'est jamais rejoué : l'agent va au bout.

## Application (code)
- `apps/desktop/src/composables/useServiceDelete.ts::perform` (`unknown`)
- `apps/desktop/src/components/organisms/DeleteProgressDialog.vue` (`data-delete-unknown`)
- `crates/hearth-link/src/manager/service_delete.rs::LinkManager::delete_service` (`execute_act`)

## Vérification
- `apps/desktop/src/pages/ServicesDelete.test.ts` (un lien coupé après l'envoi)

## Cas limites
- Aucun.

## Règles liées
- BR-RESIL-009
- BR-RESIL-010

## Historique
- 2026-11-14 : création (HRT-86, côté client).
