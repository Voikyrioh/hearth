---
id: BR-AUDIT-021
domaine: AUDIT
titre: Un refus d'accès au journal est lui-même journalisé
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-021), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-021 — Un refus d'accès au journal est lui-même journalisé

## Règle
Quand un compte lecture seule demande le journal ou son export, la réponse est `403` et une entrée est écrite : action « Tentative de lecture du journal », résultat « Refusé », raison « lecture seule », compte et origine de l'appelant, cible `/audit` ou `/audit/export`. Même règle pour toute route réservée aux administrateurs (action propre à la route : par exemple « Consultation des comptes »).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS` (colonne `audit` : `AuditAction::AuditRead`).
- `crates/hearth-agent/src/entrypoint/http/auth.rs::guard`.
- `crates/hearth-agent/src/domain/audit/action.rs::AuditAction::AuditRead`.
- `crates/hearth-agent/src/domain/audit/policy.rs::is_journaled`.

## Vérification
- `crates/hearth-agent/tests/audit_https.rs::a_read_only_account_is_refused_and_its_attempts_are_journaled`.
- `crates/hearth-agent/tests/http_api.rs::every_admin_route_refuses_a_read_only_account_and_changes_nothing` (une entrée refusée par route de la table).
- `entrypoint::http::tests::every_route_that_modifies_or_is_reserved_has_a_journal_action`.

## Cas limites
- Le message du `403` dépend de la route : celui du journal parle du journal, celui des routes de comptes de la gestion des comptes (`auth::forbidden_message`).
- Les refus identiques répétés se regroupent (BR-AUDIT-007).
- Un jeton absent ou invalide (401) n'écrit rien : l'appelant est inconnu.

## Règles liées
- BR-AUDIT-001, BR-AUDIT-004.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
