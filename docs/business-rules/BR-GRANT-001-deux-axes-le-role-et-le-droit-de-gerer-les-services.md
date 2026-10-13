---
id: BR-GRANT-001
domaine: GRANT
titre: Un compte a un rôle et, séparément, le droit « gérer les services » : deux axes indépendants, quatre combinaisons
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-001), contexts/hearth/conceptions/2026-10-13-adr-droit-de-gerer-les-services-brouillon.md, HRT-76
maj: 2026-10-13
---

# BR-GRANT-001 — Un compte a un rôle et, séparément, le droit « gérer les services » : deux axes indépendants, quatre combinaisons

## Règle
Le rôle (`Admin` ou `ReadOnly`) gouverne les comptes, le journal d'activité et les mises à jour de l'agent ; le droit « gérer les services » (`ServicesRight::{None, Manage}`) gouverne les actes de service. Les quatre combinaisons existent : administrateur avec le droit (A), administrateur sans (B), lecture seule sans (C), lecture seule avec (D). Un compte D pilote les services mais n'a accès ni aux comptes, ni au journal d'activité, ni aux mises à jour ; un compte B gère les comptes mais aucun service. Le module `rights.rs` ne dépend pas de `Role`, et la colonne `accounts.services_right` est distincte de `accounts.role` : changer un rôle ne touche jamais au droit.

HRT-76 : le niveau d'accès de route `Access::Services` ne regarde que le droit ; `Access::Admin` ne regarde que le rôle.

## Application (code)
- `crates/hearth-agent/src/domain/accounts/rights.rs::ServicesRight`
- `crates/hearth-agent/src/domain/accounts/role.rs::Role::can_manage_accounts`
- `crates/hearth-agent/src/entrypoint/http/auth.rs::allows`
- `crates/hearth-agent/migrations/0010_services_right.sql`

## Vérification
- `crates/hearth-agent/src/entrypoint/http/auth.rs::tests::only_the_right_passes_the_services_level_whatever_the_role`
- `crates/hearth-agent/tests/http_api.rs::a_read_only_account_with_the_right_reaches_nothing_but_the_services`
- `crates/hearth-agent/tests/http_api.rs::every_services_route_refuses_an_account_without_the_right_even_an_administrator`
- `crates/hearth-agent/tests/services_https.rs::only_an_account_with_the_right_reveals_the_values_whatever_its_role_and_every_attempt_is_journaled_without_them`

## Cas limites
- Un administrateur rétrogradé en lecture seule garde le droit s'il l'avait (indépendance). Test : `http_api.rs::a_created_account_is_born_without_the_right_and_rights_are_described_on_the_wire` (la promotion d'un compte qui a le droit ne l'écrit pas).
- Un compte supprimé emporte son droit avec lui (la colonne est sur la ligne du compte) ; un compte recréé du même identifiant naît sans le droit.

## Règles liées
- BR-GRANT-002, BR-GRANT-003, BR-ACCT-013, BR-ACCT-014

## Historique
- 2026-10-13 — création (HRT-76, ADR-0041 : le droit de gérer les services, moitié agent).
