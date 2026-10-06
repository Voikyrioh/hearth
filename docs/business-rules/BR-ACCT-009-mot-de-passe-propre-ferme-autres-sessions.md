---
id: BR-ACCT-009
domaine: ACCT
titre: Changer son propre mot de passe ferme les autres sessions
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-009), HRT-03
maj: 2026-10-06
---

# BR-ACCT-009 — Changer son propre mot de passe ferme les autres sessions

## Règle
Le titulaire saisit l'ancien mot de passe puis le nouveau. Les autres sessions du compte sont fermées ; la session courante est conservée. Si l'appelant n'a pas de session courante, toutes sont fermées. Message d'erreur : « L'ancien mot de passe est incorrect ». L'ancien mot de passe n'est pas soumis aux règles de complexité.

## Application (code)
- `crates/hearth-agent/src/domain/sessions.rs::closure_on_password_change` (`PasswordChange::Own { current_session }` → `SessionClosure::AllExcept` ou `All`).
- `crates/hearth-agent/src/application/accounts.rs::AccountService::change_own_password` — vérifie l'ancien (`PasswordHasher::verify`), applique les règles au nouveau, change et ferme dans une transaction.
- Route : `PUT /api/v1/me/password` (`entrypoint/http/accounts.rs::change_own_password`), permise à tout rôle.

## Interface (HRT-13)
`PasswordDialog.vue` en mode propre (ancien, nouveau, confirmation) depuis la ligne « toi » de la page Comptes et la section « Mon compte » des réglages (`OwnAccountCard.vue`, tous les rôles) → commande `change_own_password` (`PUT /me/password`). Ancien incorrect : « L'ancien mot de passe est incorrect » sous le champ. Test : `accounts_runtime.rs::changing_your_own_password_asks_for_the_old_one_and_keeps_the_current_session`.
Le mot de passe mémorisé au coffre suit (`apps/desktop/src-tauri/src/accounts/service.rs::change_own_password`, `LinkManager::take_remembered_password` / `::remember_password`) : retiré avant l'envoi ; réussi, remplacé par le nouveau (si « se souvenir ») ; refusé, remis ; résultat inconnu, effacé. Tests : `accounts_runtime.rs::after_changing_your_own_password_the_vault_follows_and_the_silent_reconnection_succeeds`, `::without_remember_nothing_is_written_to_the_vault_and_a_refusal_restores_the_old_entry`, `::an_own_password_change_cut_before_its_answer_leaves_the_vault_entry_erased_not_wrong`.

## Vérification
- Tests : `domain::sessions::tests::own_password_change_keeps_the_current_session`, `own_password_change_without_current_session_closes_everything` ; `crates/hearth-agent/tests/accounts_use_cases.rs::changing_your_own_password_keeps_only_the_current_session`, `a_wrong_old_password_changes_nothing`, `a_password_changed_between_verification_and_write_is_not_overwritten`.

## Cas limites
- Ancien mot de passe faux → rien ne change, aucune session fermée.
- Le mot de passe change entre la vérification de l'ancien et l'écriture → refus « Le mot de passe a été modifié entre-temps, réessaye », rien n'est écrasé (le haché vérifié est comparé au haché en base dans la transaction).
- Les routes HTTP (HRT-04) fourniront `current_session` ; la ligne de commande n'a pas de session et passe par `set_password`.

## Règles liées
- BR-ACCT-008, BR-ACCT-004.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » (HRT-13, session 2026-10-04-hearth-creation).
