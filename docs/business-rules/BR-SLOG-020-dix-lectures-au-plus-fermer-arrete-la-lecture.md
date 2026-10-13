---
id: BR-SLOG-020
domaine: SLOG
titre: Dix lectures simultanées au plus pour tout l'agent ; fermer le socket arrête la lecture côté serveur et rend la place
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-020), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-020 — Dix lectures simultanées au plus pour tout l'agent ; fermer le socket arrête la lecture côté serveur et rend la place

## Règle
Les dix places sont communes à tous les comptes (`ServiceLogs::acquire`, `Permit`). Une onzième ouverture répond `error { code: "LOGS_LIMIT" }`. Fermer le socket, proprement ou non (coupure du lien, client disparu), lâche la `LogSession` : la tâche qui lit le moteur est abandonnée, la connexion au moteur se ferme et la place revient. Une lecture suspendue par une coupure libère donc sa place ; à la reprise elle en retrouve une s'il en reste, sinon `LOGS_LIMIT`. La place d'attente d'authentification (quotas de `/stream`) est gardée jusqu'à l'`open`, puis rendue : une lecture est comptée par ses propres dix places. Un compte en lecture seule pouvait tenir les dix places et en priver les administrateurs : le propriétaire a décidé, le 2026-10-13, d'un plafond par compte (3 lectures, BR-SLOG-026) et d'un frein aux ouvertures (BR-SLOG-027), qui se vérifient avant celui-ci. Le texte « Trop de journaux sont ouverts… » est porté par le client.

## Application (code)
- `crates/hearth-agent/src/application/service_logs.rs::{ServiceLogs::acquire, LogSession (Drop)}`
- `crates/hearth-proto/src/api/service_logs.rs::MAX_READS`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::only_ten_readings_are_open_at_once_and_closing_one_gives_its_place_back`
- `crates/hearth-agent/tests/service_logs_ws.rs::the_place_of_a_reading_comes_back_whatever_ends_it`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_revoked_session_closes_its_reading_and_gives_the_place_back`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_silent_client_loses_its_reading_and_the_agent_shutting_down_closes_it_going_away`

## Règles liées
- BR-SLOG-007
- BR-SLOG-026
- BR-SLOG-027

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
- 2026-10-13 — corrections de la review locale de HRT-66 : la place revient par tous les chemins de sortie (fermeture, coupure, session révoquée, silence, mauvaise demande, service disparu), testés.
