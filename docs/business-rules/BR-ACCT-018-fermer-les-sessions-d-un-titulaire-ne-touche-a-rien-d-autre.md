---
id: BR-ACCT-018
domaine: ACCT
titre: Un administrateur sans le droit qui ferme les sessions d'un titulaire ne ferme que des sessions
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (section 9, BR-ACCT-018), HRT-133
maj: 2026-10-13
---

# BR-ACCT-018 — Fermer les sessions d'un titulaire ne touche à rien d'autre

## Règle
Quand l'administrateur qui agit **n'a pas** le droit de gérer les services et que le compte visé **l'a**, le geste ferme les sessions **et rien d'autre** : le mot de passe, les postes de confiance, les adresses retenues et le droit du compte visé sont gardés. Le compte visé se reconnecte avec son mot de passe depuis son poste habituel, y compris en mode attaque. Dans les autres cas (ligne de commande du serveur, administrateur qui a le droit, compte visé sans le droit), le geste garde son effet d'avant : toutes les sessions, et postes de confiance et adresses oubliés. Quand un compte ferme ses propres sessions, celle d'où il agit reste ouverte et ses postes à clé restent (comme BR-ACCT-009 ; les adresses apprises sans clé sont oubliées). Le droit est lu dans la transaction du geste.

La fenêtre de confirmation du client le dit dans le premier cas seulement : « Ce compte peut gérer les services. Il devra se reconnecter ; ses postes de confiance sont gardés. ».

Ce que cette règle ne change pas (défaut posé avec BR-GRANT-005, toujours à confirmer) : changer le mot de passe d'un titulaire reste réservé à lui-même, à un autre titulaire ou à la ligne de commande. Constat de HRT-133 : le changement de rôle et la suppression d'un titulaire par un administrateur sans le droit sont AUJOURD'HUI acceptés (classés « sans danger », `entrypoint/http/mod.rs::CLASSES`, test `the_acts_declared_harmless_do_not_hand_a_holder_to_an_administrator_without_the_right`) ; le retrait d'un poste n'existe que pour son titulaire.

## Application (code)
- `crates/hearth-agent/src/domain/sessions.rs::plan_revocation` (rend `Revocation { closure, trust }`, `TrustAfterRevocation::{ForgetAll, ForgetWithoutDevice, Keep}`).
- `crates/hearth-agent/src/application/accounts.rs::AccountService::revoke_sessions_as`.
- Client : `apps/desktop/src/pages/Accounts.vue` (`keepsTrustedDevices`).

## Vérification
- `crates/hearth-agent/src/domain/sessions.rs` (tests `an_administrator_without_the_right_closes_a_holder_and_nothing_else`, `every_other_case_keeps_the_effect_it_had_before`, `closing_ones_own_sessions_keeps_the_current_one`).
- `crates/hearth-agent/tests/http_api.rs::an_administrator_without_the_right_closes_the_sessions_of_a_holder_and_nothing_else`, `::the_other_cases_still_forget_the_trusted_devices`, `::an_administrator_without_the_right_cannot_define_the_password_of_a_holder`.
- `apps/desktop/e2e/hrt133-sessions.spec.ts`.

## Règles liées
- BR-ACCT-011, BR-ACCT-017, BR-ACCT-019, BR-GRANT-005, BR-TRUST-023, BR-TRUST-024.

## Historique
- 2026-10-13 : création (HRT-133).
