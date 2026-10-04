---
id: BR-ACCT-016
domaine: ACCT
titre: Tous les changements de compte sont consignés au journal d'activité
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-016), HRT-03
maj: 2026-10-04
---

# BR-ACCT-016 — Tous les changements de compte sont consignés au journal d'activité

## Règle
Création, suppression, changement de rôle, changement de mot de passe (le sien ou celui d'autrui) et fermeture de sessions sont consignés dans le journal d'activité avec l'identité de l'administrateur (ou l'origine « ligne de commande du serveur »), l'action, la cible et l'horodatage. **Appliquée (HRT-05)** : l'entrée est écrite **dans la transaction de l'action** (elle existe si l'action est validée, et seulement alors), puis diffusée sur le canal interne une fois la transaction validée. Une action qui échoue ne laisse aucune entrée de réussite ; son échec est consigné à part par la couche d'accès. Un échec d'écriture du journal ne fait pas échouer l'action (tracé en `error`).

## Application (code)
- `crates/hearth-agent/src/application/accounts.rs::AccountService::{create, change_role, set_password, change_own_password, delete, revoke_sessions}` : chaque méthode prend l'initiateur (`&Actor` : compte et origine) et écrit son entrée par `application/audit.rs::Pending::record`.
- `crates/hearth-agent/src/domain/audit/` : `AuditAction`, `Actor`, `Target`, `is_journaled` (BR-AUDIT-003).
- `crates/hearth-agent/src/entrypoint/http/auth.rs::Requester` (compte et origine de la requête, posés par la couche d'accès) ; `entrypoint/account.rs::execute` (origine « ligne de commande »).

## Vérification
- `crates/hearth-agent/tests/audit_use_cases.rs::account_management_writes_one_entry_per_action_with_who_where_and_what`, `::an_action_that_fails_leaves_no_success_entry`.
- `crates/hearth-agent/tests/audit_https.rs::an_account_creation_produces_its_entry_with_who_where_and_what`.
- `crates/hearth-agent/tests/account_cli.rs::every_command_is_journaled_with_the_command_line_origin_and_no_secret`.

## Cas limites
- Un secret (mot de passe, haché, jeton) ne figure jamais dans une ligne de journal : BR-AUDIT-005.
- Un changement de rôle consigne le nouveau rôle dans la cible (« paul (Administrateur) »).

## Règles liées
- BR-ACCT-015, BR-AUDIT-003, BR-AUDIT-005.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
- 2026-10-04 — appliquée (HRT-05).
