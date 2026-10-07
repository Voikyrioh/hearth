---
id: BR-AUDIT-003
domaine: AUDIT
titre: Connexions, refus, déconnexions, modifications et refus faute de droits sont journalisés
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-003), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-003 — Connexions, refus, déconnexions, modifications et refus faute de droits sont journalisés

## Règle
Sont consignés : toute connexion réussie ; toute tentative de connexion refusée ; toute déconnexion ; toute requête qui modifie (création, suppression, rôle, mots de passe, fermeture de sessions), réussie ou échouée ; toute action refusée faute de droits. Les succès sont écrits par les cas d'usage **dans la transaction de l'action** (l'entrée existe si l'action est validée, et seulement alors) ; les refus et les échecs des routes sont écrits par la couche d'accès, d'après la colonne « action de journal » de `ENDPOINTS`, sans qu'un handler y pense. Une erreur d'écriture d'une entrée écrite dans la transaction de l'action la fait échouer (pas d'action validée sans son entrée) ; hors transaction (refus, échecs), elle est tracée en `error`.

## Application (code)
- `crates/hearth-agent/src/domain/audit/policy.rs::is_journaled`.
- `crates/hearth-agent/src/application/accounts.rs::AccountService` et `application/sessions.rs::SessionService::{login, logout}` (succès, connexion refusée, blocage).
- `crates/hearth-agent/src/entrypoint/http/auth.rs::{guard, failure_of}` et `entrypoint/http/mod.rs::ENDPOINTS` (colonne `audit`).
- `crates/hearth-agent/src/entrypoint/account.rs::execute` (origine « ligne de commande »).
- `crates/hearth-agent/src/application/audit.rs::{Pending, AuditRecorder}`.

## Vérification
- `domain::audit::policy::tests::what_is_journaled_follows_the_rules_of_the_spec`.
- `crates/hearth-agent/tests/audit_use_cases.rs::account_management_writes_one_entry_per_action_with_who_where_and_what`.
- `crates/hearth-agent/tests/audit_use_cases.rs::an_action_that_fails_leaves_no_success_entry`.
- `crates/hearth-agent/tests/audit_https.rs` (création, refus, échec écrit une seule fois même rejoué).
- `crates/hearth-agent/tests/account_cli.rs::every_command_is_journaled_with_the_command_line_origin_and_no_secret`.
- `entrypoint::http::tests::every_route_that_modifies_or_is_reserved_has_a_journal_action`.

## Cas limites
- Les refus et échecs identiques répétés (même compte, action, résultat, cible et raison ; pas le nom du poste ni l'adresse) sont regroupés par fenêtre de 60 s en une entrée de synthèse (`repeat_count`) : voir BR-AUDIT-007.
- « Toute mise à jour de l'agent » : l'action `agent.update` est au catalogue, son écriture arrive avec la mise à jour de l'agent (HRT-17).
- Une requête rejouée depuis sa clé d'opération ne s'est pas exécutée : rien n'est écrit une seconde fois.
- Sans jeton valable (401), l'appelant est inconnu : rien n'est écrit.

## Règles liées
- BR-AUDIT-004, BR-AUDIT-007, BR-AUDIT-021, BR-ACCT-016, BR-RESIL-010.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
- 2026-10-07 : HRT-28 : nouvelle action `reauth.setting` ; refus de confirmation consignés sous l'action de l'acte (BR-TRUST-046).
