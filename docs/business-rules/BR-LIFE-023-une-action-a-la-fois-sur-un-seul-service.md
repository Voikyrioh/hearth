---
id: BR-LIFE-023
domaine: LIFE
titre: Les actions portent sur un seul service à la fois : il n'y a pas d'action groupée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-023), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-023 — Les actions portent sur un seul service à la fois : il n'y a pas d'action groupée

## Règle
Chaque route prend UNE clé de service dans son chemin ; il n'existe aucune route qui pilote plusieurs services. Des actions sur des services différents sont indépendantes (le verrou est par service, BR-LIFE-013).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS (`/services/{key}/start|stop|restart`)`

## Vérification
- `crates/hearth-agent/tests/service_lifecycle_https.rs::a_second_request_on_a_busy_service_is_refused_with_who_holds_it_and_other_services_are_free`

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux de la règle.

## Règles liées
- BR-LIFE-013

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
