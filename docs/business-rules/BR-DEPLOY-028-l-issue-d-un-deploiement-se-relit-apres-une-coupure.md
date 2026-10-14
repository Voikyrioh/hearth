---
id: BR-DEPLOY-028
domaine: DEPLOY
titre: L'issue d'un déploiement se relit après une coupure du lien, par la même route que toute opération de service
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 0 bis), contexts/hearth/tickets/hrt/HRT-144.md
maj: 2026-10-14
---

# BR-DEPLOY-028 — L'issue d'un déploiement se relit après une coupure du lien, par la même route que toute opération de service

## Règle
Un déploiement est une opération de service (genre `deploy`) : son issue se relit comme celle de toutes les autres, par `GET /operations/services/{id}` pendant 24 heures après sa fin, et arrive par le message `operation` du flux. Les routes qui créent le déploiement (HRT-68) rendent `202 { op_id }` ; l'`op_id` est ce que le client garde pour relire.

## Application (code)
- `crates/hearth-agent/src/application/service_op_access.rs::ServiceOpAccess::read`
- `crates/hearth-agent/src/entrypoint/http/service_ops.rs::get`

## Vérification
- `crates/hearth-agent/tests/service_ops_https.rs::any_account_reads_an_operation_of_a_hearth_service_with_its_steps_and_progress` (le genre est celui de l'opération)

## Cas limites
- La route qui crée un déploiement n'existe pas encore (HRT-68) : cette fiche s'applique à elle dès qu'elle existera ; aujourd'hui seule la relecture existe.

## Règles liées
- BR-LIFE-020, BR-OPS-001

## Historique
- 2026-10-14 — création (HRT-144 : le socle des opérations branché sur des routes et sur le flux).
