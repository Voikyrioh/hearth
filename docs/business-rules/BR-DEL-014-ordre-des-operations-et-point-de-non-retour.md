---
id: BR-DEL-014
domaine: DEL
titre: Verrou, arrêt, conteneur, description, puis données ; le conteneur est le point de non-retour
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-014), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-014 : Verrou, arrêt, conteneur, description, puis données ; le conteneur est le point de non-retour

## Règle
Dans l'ordre : verrou du service (et nom réservé), arrêt, suppression du conteneur (POINT DE NON-RETOUR : jusqu'à son `intent`, l'annulation est possible et ne laisse aucune trace), oubli de la description, puis effacement des dossiers (si la case est cochée). L'effacement vient APRÈS le conteneur, jamais avant : un échec laisse du superflu sur le disque, jamais un service à moitié vidé. Chaque étape qui détruit écrit son `intent` avant le geste.

## Application (code)
- `crates/hearth-agent/src/domain/ops/delete.rs::{op_plan, step}`
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::run`

## Vérification
- `crates/hearth-agent/src/domain/ops/delete.rs::tests`
- `crates/hearth-agent/tests/service_delete_https.rs::the_data_goes_only_after_the_container_while_the_name_is_reserved_and_the_service_busy`

## Cas limites
- Aucun.

## Règles liées
- BR-DEL-013
- BR-DEL-015
- BR-DEL-025

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
