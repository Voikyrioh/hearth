---
id: BR-OPS-007
domaine: OPS
titre: Le journal garde au plus 500 opérations terminées, et 24 heures ; une opération non terminée n'est jamais purgée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0 bis et 7), contexts/hearth/tickets/hrt/HRT-144.md
maj: 2026-10-14
---

# BR-OPS-007 — Le journal garde au plus 500 opérations terminées, et 24 heures ; une opération non terminée n'est jamais purgée

## Règle
Le balayage périodique de l'agent efface les opérations terminées depuis plus de 24 heures, PUIS ne garde que les 500 opérations terminées les plus récentes (avec leur journal d'étapes). Une opération qui n'est pas terminée (en cours, en attente, à vérifier) n'est jamais comptée ni effacée.

## Application (code)
- `crates/hearth-agent/src/application/service_ops.rs::ServiceOps::sweep`
- `crates/hearth-agent/src/infrastructure/sqlite/ops_repo.rs::SqliteOpJournal::{purge_finished, prune_finished_beyond}`
- `crates/hearth-proto/src/api/service_ops.rs::{MAX_RETAINED_OPERATIONS, FINISHED_RETENTION_S}`

## Vérification
- `crates/hearth-agent/tests/service_ops_https.rs::the_number_of_kept_operations_is_bounded_and_an_open_one_is_never_dropped`
- `crates/hearth-agent/tests/service_ops_journal.rs` (purge à 24 heures)

## Cas limites
- Le balayage tourne toutes les 30 secondes : le plafond est atteint à une période près.

## Règles liées
- BR-LIFE-020, BR-JEU-006

## Historique
- 2026-10-14 — création (HRT-144 : le socle des opérations branché sur des routes et sur le flux).
