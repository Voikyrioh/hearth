---
id: BR-ACCT-014
domaine: ACCT
titre: Le serveur refuse la gestion de comptes à un compte lecture seule
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-014), HRT-03
maj: 2026-10-04
---

# BR-ACCT-014 — Le serveur refuse la gestion de comptes à un compte lecture seule

## Règle
Même si le client est modifié ou contourné, l'agent refuse création, suppression, changement de rôle, changement du mot de passe d'autrui et révocation par un compte lecture seule. Le contrôle est fait par l'agent, jamais par le client.

## Application (code)
- `crates/hearth-agent/src/domain/accounts/role.rs::Role::can_manage_accounts` — décision pure.
- `crates/hearth-agent/src/entrypoint/http/auth.rs::AdminOnly` — une seule couche d'extraction, avant la lecture du corps ; `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS` — table de toutes les routes, dont le niveau d'accès est vérifié par le balayage.

## Vérification
- Tests : `domain::accounts::role::tests::only_an_administrator_manages_accounts` ; `tests/http_api.rs::every_admin_route_refuses_a_read_only_account_and_changes_nothing` (chaque route modifiante réservée : `403` pour lecture seule, rien ne change en base) et `::every_reserved_route_refuses_a_caller_without_a_session` (`401`) ; `::no_route_exists_outside_the_endpoint_table`.

## Cas limites
- La ligne de commande n'a pas de rôle : elle s'exécute avec les droits du système sur le serveur (BR-ACCT-015).

## Règles liées
- BR-ACCT-013, BR-ACCT-015.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
- 2026-10-04 — appliquée aux routes (HRT-04).
