---
id: BR-LIFE-015
domaine: LIFE
titre: Si le service n'existe plus au moment de l'action, Hearth le dit, rien n'est fait
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-015), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-015 — Si le service n'existe plus au moment de l'action, Hearth le dit, rien n'est fait

## Règle
Une clé inconnue, mal formée ou non publiée reçoit `404 SERVICE_NOT_FOUND` « Ce service n'existe plus. », identique pour les trois : rien ne dit qu'un conteneur non adopté existe (BR-LIFE-024). Un service qui disparaît PENDANT l'action (le moteur répond « inconnu ») termine l'opération en échec avec la cause `service_not_found`. Aucun ordre n'est donné à une clé inconnue. Le refus est au journal d'activité (« service introuvable »).

## Application (code)
- `crates/hearth-agent/src/application/service_lifecycle.rs::{ServiceLifecycle::request, OrderFailure::Gone}`
- `crates/hearth-agent/src/entrypoint/http/services.rs::{lifecycle, lifecycle_error}`

## Vérification
- `crates/hearth-agent/tests/service_lifecycle_https.rs::an_unknown_a_malformed_and_a_hidden_service_are_all_answered_the_same_way`

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux de la règle.

## Règles liées
- BR-LIFE-024, BR-ADOPT-004

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
