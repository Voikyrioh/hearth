---
id: BR-ACCT-013
domaine: ACCT
titre: Un compte lecture seule n'a pas accès à la gestion des comptes
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-013), HRT-03
maj: 2026-10-06
---

# BR-ACCT-013 — Un compte lecture seule n'a pas accès à la gestion des comptes

## Règle
Un compte lecture seule ne voit pas la gestion des comptes ; il peut uniquement changer son propre mot de passe. La règle « qui peut gérer les comptes » est une fonction pure du rôle, que la garde de HRT-04 appelle. Message : « Tu n'as pas la permission pour accéder à la gestion des comptes ».

## Application (code)
- `crates/hearth-agent/src/domain/accounts/role.rs::Role::can_manage_accounts`.
- `crates/hearth-agent/src/entrypoint/http/auth.rs::guard` — couche unique posée par le routeur sur les routes de niveau `Access::Admin` de `ENDPOINTS` ; refuse `403 FORBIDDEN_ROLE` (« Tu n'as pas la permission pour accéder à la gestion des comptes »).

## Interface (HRT-13)
Aucune entrée « Comptes » dans `ServerNav.vue` et route fermée par `router/index.ts::redirectFor` (`adminOnly`) ; la section personnelle des réglages reste ouverte à tous les rôles. Tests : `Accounts.test.ts::has no « Comptes » entry in the menu of a read-only account`, `e2e/accounts.spec.ts` (lecture seule).
Un compte Lecture seule qui change son mot de passe n'envoie jamais `GET /accounts` (aucun refus consigné pour un parcours permis) : `AccountsReview.test.ts`, `accounts_runtime.rs::a_read_only_account_changing_its_own_password_leaves_no_denial_in_the_journal`.

## Vérification
- Tests : `domain::accounts::role::tests::only_an_administrator_manages_accounts` ; `tests/http_api.rs::every_admin_route_refuses_a_read_only_account_and_changes_nothing` (balayage de `ENDPOINTS`) ; `tests/sessions_https.rs::a_read_only_account_cannot_manage_accounts_over_tls`.

## Cas limites
- Le changement de son propre mot de passe (BR-ACCT-009) reste permis à tout rôle.

## Règles liées
- BR-ACCT-014.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
- 2026-10-04 — appliquée aux routes (HRT-04).
- 2026-10-06 — section « Interface » (HRT-13, session 2026-10-04-hearth-creation).
