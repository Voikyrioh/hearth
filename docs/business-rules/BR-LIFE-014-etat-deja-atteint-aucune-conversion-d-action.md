---
id: BR-LIFE-014
domaine: LIFE
titre: Une demande sur un service déjà dans l'état visé n'a aucun effet et le dit ; Hearth ne convertit jamais un geste en un autre
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-014), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-014 — Une demande sur un service déjà dans l'état visé n'a aucun effet et le dit ; Hearth ne convertit jamais un geste en un autre

## Règle
Démarrer un service qui tourne, arrêter un service arrêté : `409 SERVICE_ALREADY_IN_STATE` (`details.state`). Redémarrer un service arrêté, ou toute action sur un service en pause, en arrêt ou non reconnu : `409 SERVICE_NOT_DRIVABLE` (`details.state`) ; redémarrer un service arrêté n'est jamais converti en démarrage. Refus SYNCHRONES : aucune opération n'est acceptée, aucun ordre n'est donné. Chaque refus est inscrit au journal d'activité (« déjà dans l'état demandé » ou « état du service incompatible avec l'action », résultat « Refusé »). Les textes sont fixes et ne répètent ni la clé ni le nom (le client compose « Le service {nom} est déjà arrêté. »).

## Application (code)
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::check_state`
- `crates/hearth-agent/src/entrypoint/http/services.rs::lifecycle_error`
- `crates/hearth-agent/src/entrypoint/http/auth.rs::failure_of`

## Vérification
- `crates/hearth-agent/tests/service_lifecycle_https.rs::a_request_the_state_does_not_allow_is_refused_at_once_with_a_typed_code_and_nothing_is_ordered`
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::tests::restarting_a_stopped_service_is_never_turned_into_a_start`

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux de la règle.

## Règles liées
- BR-LIFE-003, BR-LIFE-018

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
