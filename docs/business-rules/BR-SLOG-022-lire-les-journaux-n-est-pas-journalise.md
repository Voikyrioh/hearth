---
id: BR-SLOG-022
domaine: SLOG
titre: Lire les journaux d'un service est une consultation : elle n'est pas inscrite au journal d'activité
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-022), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-022 — Lire les journaux d'un service est une consultation : elle n'est pas inscrite au journal d'activité

## Règle
La route `GET /services/{key}/logs` n'a pas d'action d'audit dans `ENDPOINTS` (`audit: None`), et le socket n'écrit ni dans le journal ni dans la file des opérations, ni à l'ouverture, ni au refus.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS`
- `crates/hearth-agent/src/entrypoint/ws/service_logs.rs::serve`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::reading_the_logs_writes_nothing_to_the_journal_or_the_operations_queue`

## Règles liées
- BR-SLOG-019, BR-SVC-024

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
