---
id: BR-SLOG-016
domaine: SLOG
titre: Quand le moteur n'est pas prêt, la lecture ne s'ouvre pas (ENGINE_UNAVAILABLE) ; une panne en cours de lecture la termine de la même façon
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-016), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-016 — Quand le moteur n'est pas prêt, la lecture ne s'ouvre pas (ENGINE_UNAVAILABLE) ; une panne en cours de lecture la termine de la même façon

## Règle
À l'ouverture, si l'état du moteur n'est pas « Prêt » (BR-ENGINE-002), l'agent répond `error { code: "ENGINE_UNAVAILABLE" }` et ferme. Si le moteur devient injoignable, lent, ou refuse pendant la lecture, elle se termine par le même code. L'état du moteur expliqué, les lignes grisées et la reprise automatique au retour du moteur (avec le rattrapage de BR-SLOG-014) sont portés par le client.

## Application (code)
- `crates/hearth-agent/src/application/service_logs.rs::ServiceLogs::open`
- `crates/hearth-agent/src/application/service_logs.rs::Reader::follow_container`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::unreadable_logs_an_unknown_service_and_a_missing_engine_are_typed_errors`
- `crates/hearth-agent/tests/service_logs_ws.rs::an_engine_that_cannot_read_the_logs_ends_the_reading_with_a_typed_error`

## Règles liées
- BR-ENGINE-002, BR-SLOG-014

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
