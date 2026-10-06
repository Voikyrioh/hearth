---
id: BR-AUDIT-015
domaine: AUDIT
titre: Compte, action et résultat acceptent plusieurs valeurs ; une période à la fois
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-015), HRT-05
maj: 2026-10-06
---

# BR-AUDIT-015 — Compte, action et résultat acceptent plusieurs valeurs ; une période à la fois

## Règle
`account`, `action` et `outcome` acceptent plusieurs valeurs séparées par des virgules (OU à l'intérieur d'un filtre, ET entre filtres). La période est `from` et `to` (RFC 3339, bornes incluses) ; `to` avant `from` est refusé (`422`, `details.field = "to"`). Les raccourcis « Aujourd'hui », « 7 jours », « 30 jours » sont calculés par l'interface. Un code d'action ou de résultat inconnu est refusé (`422`).

## Application (code)
- `crates/hearth-agent/src/domain/audit/filter.rs::AuditFilter::new`.
- `crates/hearth-agent/src/infrastructure/sqlite/audit_repo.rs::SqliteAuditRepo::search`.
- `crates/hearth-agent/src/entrypoint/http/audit.rs::filter_of`.
- `crates/hearth-link/src/domain/audit_query.rs::{AuditFilter::new, AuditFilter::plan, merge_pages}`
- `apps/desktop/src-tauri/src/audit.rs::{AuditFilterDto, AuditKindDto}`
- `apps/desktop/src/audit/filters.ts::{periodError, resolveFilter}`

## Interface
Compte, type d'action et résultat sont des listes à choix multiple (ET entre filtres, OU dans un filtre) ; une seule période est active (« Tout l'historique », « Aujourd'hui », « 7 derniers jours », « 30 derniers jours », « Personnalisée » avec « Du » et « Au », fin incluse). Les types d'action de la spec couplent une action et un résultat : une requête par rectangle (actions × résultats) est envoyée à l'agent et les pages sont recollées sans doublon ni perte (ADR-0019). Messages : « La fin de la période doit suivre le début », « La date dépasse l'historique conservé (90 jours) ». Les comptes proposés sont ceux que le journal a montrés.

## Vérification
- `domain::audit::filter::tests`.
- `crates/hearth-agent/tests/audit_repo.rs::filters_combine_with_and_between_filters_and_or_inside_one`.
- `crates/hearth-agent/tests/audit_repo.rs::the_period_is_inclusive_at_both_ends`.
- `crates/hearth-agent/tests/audit_https.rs::the_journal_is_filtered_searched_and_paged_over_the_wire`.
- `crates/hearth-link/src/domain/audit_query.rs` (tests unitaires : validation, plan, fusion, dates, encodage).
- `crates/hearth-link/tests/audit.rs::filters_combine_by_and_and_each_filter_is_exact`, `the_period_bounds_are_inclusive_and_in_utc`.
- `apps/desktop/src-tauri/tests/audit_runtime.rs::an_invalid_filter_or_cursor_is_refused_before_anything_is_sent`.
- `apps/desktop/src/audit/audit.test.ts` (« filtres »).

## Cas limites
- Au plus 64 valeurs par filtre. L'historique plus ancien que 90 jours n'existe plus : une période qui l'englobe se comporte comme un filtre sans résultat.

## Règles liées
- BR-AUDIT-014, BR-AUDIT-016.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » et pointeurs du client (HRT-14, session 2026-10-04-hearth-creation).
