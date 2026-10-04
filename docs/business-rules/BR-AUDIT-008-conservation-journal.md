---
id: BR-AUDIT-008
domaine: AUDIT
titre: Le journal est conservé 90 jours ou 50 000 entrées
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-008), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-008 — Le journal est conservé 90 jours ou 50 000 entrées

## Règle
Une entrée est supprimée automatiquement quand elle a plus de 90 jours, puis, s'il reste plus de 50 000 entrées, les plus anciennes sont supprimées jusqu'à 50 000 : la première limite atteinte joue. Le nettoyage tourne toutes les heures avec le reste de la maintenance, **par lots de 1 000 lignes, chacun dans sa propre transaction** (il ne tient jamais longtemps le verrou d'écriture). Le plafond est **aussi contrôlé par l'écriture** : toutes les 500 entrées écrites (une transaction validée), un comptage bon marché et, s'il y a surplus, la suppression des plus anciennes par lots dans des transactions à part ; entre deux purges horaires, la table ne dépasse donc pas le plafond de beaucoup.

## Application (code)
- `crates/hearth-agent/src/domain/audit/policy.rs::{RETENTION, MAX_ENTRIES, retention_cutoff, excess_entries}`.
- `crates/hearth-agent/src/application/maintenance.rs::MaintenanceService::purge`.
- `crates/hearth-agent/src/infrastructure/sqlite/audit_repo.rs` (`purge_before`, `purge_oldest`, `note_writes`, `trim`) ; `domain/audit/policy.rs::{PURGE_BATCH, CAP_CHECK_EVERY}`.
- `crates/hearth-agent/src/entrypoint/tasks.rs::spawn_purge`.

## Vérification
- `domain::audit::policy::tests`.
- `crates/hearth-agent/tests/audit_use_cases.rs::the_purge_goes_by_batches_and_removes_everything_that_is_due`, `::writing_checks_the_cap_between_two_purges`.
- `crates/hearth-agent/tests/audit_use_cases.rs::the_hourly_purge_removes_by_age_then_by_count`.
- `crates/hearth-agent/tests/audit_use_cases.rs::the_purge_keeps_entries_of_exactly_ninety_days_and_removes_older_ones`.
- `crates/hearth-agent/tests/audit_repo.rs::the_purge_removes_by_age_and_then_by_count_and_keeps_the_search_in_step`.

## Cas limites
- Index : seul `audit_events_at` existe (purge par âge) ; la lecture, à filtres facultatifs, fait `SCAN` dans l'ordre de la clé primaire (mesuré par `EXPLAIN QUERY PLAN`), les index sur compte, action et résultat ne servaient à rien.
- Une entrée de 90 jours pile est conservée ; plus vieille d'une milliseconde, elle part.
- Entre deux passages, le journal peut dépasser un peu 50 000 entrées.

## Règles liées
- BR-AUDIT-019.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
