---
id: BR-SLOG-015
domaine: SLOG
titre: Quand le service disparaît du serveur pendant la lecture, la lecture se termine par SERVICE_NOT_FOUND
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-015), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-015 — Quand le service disparaît du serveur pendant la lecture, la lecture se termine par SERVICE_NOT_FOUND

## Règle
L'agent relit la clé à chaque tour ; si elle ne désigne plus rien (conteneur supprimé sans unité qui le garde dans la liste), il envoie `error { code: "SERVICE_NOT_FOUND" }` puis ferme. Un service recréé sous le même nom est une AUTRE clé (`c_...`, BR-SVC-016) : la lecture n'est jamais basculée dessus. Les lignes grisées et le texte « Ce service n'existe plus » sont portés par le client.

## Application (code)
- `crates/hearth-agent/src/application/service_logs.rs::Reader::run`
- `crates/hearth-agent/src/application/service_logs.rs::LogAccess::target`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::a_service_that_disappears_during_the_reading_ends_it_with_service_not_found`
- `crates/hearth-agent/tests/service_logs_ws.rs::unreadable_logs_an_unknown_service_and_a_missing_engine_are_typed_errors`

## Règles liées
- BR-SVC-016

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
