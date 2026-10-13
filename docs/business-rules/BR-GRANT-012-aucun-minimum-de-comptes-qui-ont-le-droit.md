---
id: BR-GRANT-012
domaine: GRANT
titre: Il n'existe aucun minimum de comptes qui ont le droit : un serveur peut n'en avoir aucun
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-012), contexts/hearth/conceptions/2026-10-13-adr-droit-de-gerer-les-services-brouillon.md, HRT-76
maj: 2026-10-13
---

# BR-GRANT-012 — Il n'existe aucun minimum de comptes qui ont le droit : un serveur peut n'en avoir aucun

## Règle
Contrairement aux administrateurs (`BR-ACCT-007`), aucune règle de l'agent n'impose qu'un compte ait le droit. Après la migration et tant que le ticket HRT-77 n'est pas livré, AUCUN compte ne l'a : la section des services reste consultable (liste, états, ports) et plus personne ne peut révéler les valeurs des variables. C'est le comportement sûr voulu. Aucun garde-fou du type « dernier titulaire » n'existe, ni dans le domaine ni en base.

## Application (code)
- `crates/hearth-agent/src/domain/accounts/rights.rs::ServicesRight`
- `crates/hearth-agent/src/domain/accounts/admin_guard.rs::check_removal (ne concerne que les administrateurs)`

## Vérification
- `crates/hearth-agent/tests/migration_0010.rs::no_existing_account_gets_the_right_not_even_the_first_administrator`
- `crates/hearth-agent/tests/services_https.rs::both_roles_get_the_same_engine_and_services_sorted_by_name_stopped_ones_included`
- `crates/hearth-agent/tests/services_https.rs::a_withdrawal_takes_effect_on_the_next_request_without_closing_the_session`

## Cas limites
- Supprimer le dernier compte qui a le droit n'est pas bloqué.

## Règles liées
- BR-GRANT-002, BR-ACCT-007

## Historique
- 2026-10-13 — création (HRT-76, ADR-0041 : le droit de gérer les services, moitié agent).
