---
id: BR-LIFE-003
domaine: LIFE
titre: Les gestes proposés dépendent de l'état du service : démarrer un service arrêté, arrêter un service qui tourne, redémarrer un service en marche
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-003), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-003 — Les gestes proposés dépendent de l'état du service : démarrer un service arrêté, arrêter un service qui tourne, redémarrer un service en marche

## Règle
Table pure `check_state` : démarrer demande « Arrêté » ou « En erreur » ; arrêter demande « En marche », « Démarrage » ou « Redémarre en boucle » (arrêter l'emporte sur la boucle) ; redémarrer demande « En marche » ou « Redémarre en boucle ». En pause, « Arrêt en cours » et « Non reconnu » ne se pilotent jamais. La table est la source unique : la route la lit, le client en tire ses boutons.

## Application (code)
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::{check_state, StateRefusal}`

## Vérification
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::tests::the_state_table_of_the_three_gestures`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::a_request_the_state_does_not_allow_is_refused_at_once_with_a_typed_code_and_nothing_is_ordered`

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux de la règle.

## Règles liées
- BR-LIFE-014, BR-SVC-004

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
