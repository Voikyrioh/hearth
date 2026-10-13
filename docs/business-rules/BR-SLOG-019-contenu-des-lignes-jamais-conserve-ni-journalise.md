---
id: BR-SLOG-019
domaine: SLOG
titre: Le contenu des lignes ne sort jamais de la mémoire d'une lecture : ni trace, ni base, ni journal d'activité, ni file des opérations
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-019), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-019 — Le contenu des lignes ne sort jamais de la mémoire d'une lecture : ni trace, ni base, ni journal d'activité, ni file des opérations

## Règle
L'anneau d'une lecture est le seul endroit où des lignes attendent ; il meurt avec la lecture. Aucun `Debug` d'un type qui porte une ligne n'en montre le texte (`LogLine`, `LogItem` : sa longueur seulement) ; aucune trace n'en cite ; l'adaptateur du moteur écrit son transport à la main et ne trace rien ; les cibles `bollard`, `hyper`, `tungstenite` restent sous `warn` quel que soit `RUST_LOG`. Les traces de la lecture ne portent que des nombres (ouverture, fermeture, lignes perdues). Côté client (jamais écrit sur le disque du PC, effacé à la fermeture) : HRT-67.

## Application (code)
- `crates/hearth-agent/src/domain/services/logs.rs::LogLine (Debug)`
- `crates/hearth-proto/src/api/service_logs.rs::LogItem (Debug)`
- `crates/hearth-agent/src/infrastructure/logging.rs::TransportGuard`
- `crates/hearth-agent/src/entrypoint/ws/service_logs.rs::serve`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::no_line_reaches_a_trace_the_database_the_journal_or_the_operations_queue (sentinelle, chemin d'initialisation de la production, niveau le plus bavard ; mutation vérifiée à la main)`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::the_text_of_a_line_never_appears_in_debug`
- `crates/hearth-proto/src/api/service_logs.rs::tests::the_text_of_a_line_never_appears_in_debug`

## Règles liées
- BR-SLOG-021, BR-SLOG-022

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
