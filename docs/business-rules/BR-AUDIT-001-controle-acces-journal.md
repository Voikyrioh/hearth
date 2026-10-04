---
id: BR-AUDIT-001
domaine: AUDIT
titre: Seul un administrateur lit le journal
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-001), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-001 — Seul un administrateur lit le journal

## Règle
Seul un compte au rôle Administrateur lit le journal d'activité (liste, export, flux). Une demande d'un compte lecture seule est refusée (`403 FORBIDDEN_ROLE`) et le refus est journalisé (BR-AUDIT-021).

## Application (code)
- `crates/hearth-agent/src/domain/audit/policy.rs::can_read_journal`.
- `crates/hearth-agent/src/application/audit.rs::AuditService::{search, export, subscribe}` (contrôle du rôle dans le cas d'usage aussi, pas seulement dans la table).
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS` (`GET /audit`, `GET /audit/export` : `Access::Admin`) et `entrypoint/http/auth.rs::guard`.

## Vérification
- `domain::audit::policy::tests::only_an_administrator_reads_the_journal`.
- `crates/hearth-agent/tests/audit_use_cases.rs::only_an_administrator_reads_the_journal`.
- `crates/hearth-agent/tests/http_api.rs::every_admin_route_refuses_a_read_only_account_and_changes_nothing` (balayage de la table).
- `crates/hearth-agent/tests/audit_https.rs::a_read_only_account_is_refused_and_its_attempts_are_journaled`.

## Cas limites
- Le rôle est relu à chaque requête : un administrateur rétrogradé perd l'accès tout de suite.

## Règles liées
- BR-ACCT-013, BR-ACCT-014, BR-AUDIT-021.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
