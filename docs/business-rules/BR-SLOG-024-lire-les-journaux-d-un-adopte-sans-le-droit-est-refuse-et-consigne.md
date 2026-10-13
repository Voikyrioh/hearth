---
id: BR-SLOG-024
domaine: SLOG
titre: Ouvrir les journaux d'un service adopté sans le droit de gérer les services est refusé en clair (`MISSING_RIGHT`) et consigné au journal d'activité (« Journaux lus »), sans créer d'oracle pour un conteneur caché
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-024), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-13
---

# BR-SLOG-024 — Ouvrir les journaux d'un service adopté sans le droit de gérer les services est refusé en clair (`MISSING_RIGHT`) et consigné au journal d'activité (« Journaux lus »), sans créer d'oracle pour un conteneur caché

## Règle
Un compte sans le droit qui ouvre `GET /services/{key}/logs` pour un service ADOPTÉ (qu'il voit dans la liste) reçoit `error { MISSING_RIGHT }` puis la fermeture 1000, à l'ouverture comme quand le droit lui est retiré pendant la lecture (décision rejouée toutes les 2 s). Le refus est inscrit une fois par lecture refusée : action `service.logs.read` (« Journaux lus »), résultat « Refusé : droits insuffisants », cible « service {nom} ({unité}) » (déjà publié à ce compte), compte, origine. Lire reste une consultation non consignée (BR-SLOG-022). **Aucun oracle** : `MISSING_RIGHT` ne sort que de la décision `may_read` pour l'origine « adopté », donc pour un service PUBLIÉ ; un conteneur caché, non adopté ou inconnu reçoit `SERVICE_NOT_FOUND`, mot pour mot la même séquence, avant tout appel au moteur, et n'écrit RIEN au journal.

## Application (code)
- `crates/hearth-agent/src/domain/services/logs.rs::{may_read, ReadRefusal}`
- `crates/hearth-agent/src/application/service_logs.rs::LogAccess::target`
- `crates/hearth-agent/src/entrypoint/ws/service_logs.rs::journal_refusal`
- `crates/hearth-proto/src/api/service_logs.rs::LogsErrorCode`

## Vérification
- `crates/hearth-agent/tests/service_adoption_https.rs::an_adopted_service_read_without_the_right_is_refused_in_plain_words_and_journaled_while_a_hidden_container_stays_an_unknown_service`
- `crates/hearth-agent/tests/service_adoption_https.rs::a_right_withdrawn_during_a_reading_ends_it_with_the_explicit_refusal_and_the_entry`
- `crates/hearth-agent/tests/service_logs_ws.rs::an_adopted_service_is_read_with_the_right_to_manage_services_whatever_the_role`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_right_withdrawn_during_a_reading_closes_it_with_the_explicit_refusal`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::the_verdict_depends_on_who_created_the_service_and_on_the_right_of_the_account`
- `crates/hearth-agent/tests/services_https.rs::nothing_of_a_container_that_is_not_adopted_leaves_the_agent_by_any_route_message_or_trace`

## Cas limites
- Mutation vérifiée : renvoyer `MISSING_RIGHT` pour un conteneur caché fait tomber le test des journaux d'un adopté sans le droit (la séquence du caché n'est plus celle d'une clé inconnue).

## Règles liées
- BR-SLOG-011, BR-SLOG-018, BR-SLOG-022, BR-ADOPT-004, BR-ADOPT-015, BR-GRANT-004

## Historique
- 2026-10-13 — création (HRT-80, ADR-0043 : l'acte d'adopter et de relâcher, moitié agent et protocole).
