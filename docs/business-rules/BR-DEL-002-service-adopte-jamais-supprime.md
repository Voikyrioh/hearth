---
id: BR-DEL-002
domaine: DEL
titre: Hearth ne supprime jamais un service adopté ou créé hors de Hearth
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-002), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-11-14
---

# BR-DEL-002 : Hearth ne supprime jamais un service adopté ou créé hors de Hearth

## Règle
Une clé qui n'est pas celle d'un service créé par Hearth (`h_`) n'est jamais supprimée : si elle est publiée (donc adoptée), la demande reçoit `409 SERVICE_ADOPTED` « Hearth ne supprime pas les services créés hors de Hearth. Tu peux seulement ne plus le gérer. », rien n'est touché (ni le moteur, ni la base, ni les fichiers) et le refus est inscrit au journal d'activité avec la cause « service adopté » et le nom du service (déjà publié à tous). Une clé non publiée est un service inconnu (BR-ADOPT-004). Vaut pour la fenêtre comme pour l'acte.

## Application (code)
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::subject`
- `crates/hearth-agent/src/entrypoint/http/auth.rs::failure_of` (cause `Reason::ServiceAdopted`)
- `apps/desktop/src/services/delete.ts::deleteDoor` (`adopted` : une phrase, aucun bouton)
- `apps/desktop/src/components/molecules/DeleteServiceCard.vue`
- `apps/desktop/src/services/delete.ts::refusalText` (`service_adopted`)

## Vérification
- `crates/hearth-agent/tests/service_delete_https.rs::an_adopted_service_is_never_deleted_and_the_refusal_is_journaled`
- `apps/desktop/src/pages/ServicesDelete.test.ts` (un service adopté ne propose que la phrase)
- `apps/desktop/src/link/simulated-delete.test.ts`

## Cas limites
- Le client n'offre pas « Supprimer » pour un service adopté (HRT-86) ; ce refus vaut pour un client modifié.

## Règles liées
- BR-ADOPT-004
- BR-DEL-001

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
- 2026-11-14 : côté client (HRT-86).
