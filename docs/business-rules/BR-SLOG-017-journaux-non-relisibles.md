---
id: BR-SLOG-017
domaine: SLOG
titre: Un service dont le moteur ne permet pas de relire les journaux répond LOGS_UNREADABLE, sans contournement
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-017), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-017 — Un service dont le moteur ne permet pas de relire les journaux répond LOGS_UNREADABLE, sans contournement

## Règle
Le pilote de journaux `none` est connu d'avance par l'inspection (`Observation.log_driver`) : l'ouverture répond `error { code: "LOGS_UNREADABLE" }`. Sinon, la réponse 501 du moteur à la lecture est traduite de même (`EngineError::Unsupported`). Hearth ne lit jamais journald ni un fichier à la place du moteur.

## Application (code)
- `crates/hearth-agent/src/domain/services/logs.rs::LogContainer::from_observation`
- `crates/hearth-agent/src/infrastructure/engine/logs.rs::Reader::open_over`
- `crates/hearth-agent/src/application/ports/container_engine.rs::EngineError::Unsupported`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::unreadable_logs_an_unknown_service_and_a_missing_engine_are_typed_errors`
- `crates/hearth-agent/tests/service_logs_ws.rs::an_engine_that_cannot_read_the_logs_ends_the_reading_with_a_typed_error`
- `crates/hearth-agent/src/infrastructure/engine/logs.rs::tests::the_engine_statuses_are_translated_and_its_text_is_dropped`
- job `engine` de la CI : `tests/support/engine_contract.rs::logs` (pilote `none`) contre un vrai moteur

## Règles liées
- BR-SLOG-021

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
