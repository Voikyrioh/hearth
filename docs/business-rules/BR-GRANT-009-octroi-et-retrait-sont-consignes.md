---
id: BR-GRANT-009
domaine: GRANT
titre: Chaque octroi ou retrait effectif est consigné avec l'origine « ligne de commande du serveur », avant d'être annoncé ; une commande sans effet ou refusée n'ajoute rien
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-009), contexts/hearth/conceptions/2026-10-13-adr-droit-de-gerer-les-services-brouillon.md, HRT-77
maj: 2026-10-13
---

# BR-GRANT-009 — Chaque octroi ou retrait effectif est consigné avec l'origine « ligne de commande du serveur », avant d'être annoncé ; une commande sans effet ou refusée n'ajoute rien

## Règle
Chaque octroi ou retrait EFFECTIF ajoute une entrée au journal d'activité : action « Droit de gérer les services accordé » (`account.services.grant`) ou « Droit de gérer les services retiré » (`account.services.revoke`), cible : le compte, origine : « ligne de commande du serveur » (`cli`, sans compte acteur ni adresse), résultat « Réussi ». L'octroi fait par l'installation est consigné de la même façon. Le changement et son entrée sont écrits dans la MÊME transaction : le message de réussite n'est dit qu'après leur validation commune, et un échec d'écriture du journal annule le changement. Une commande sans effet (déjà dans l'état demandé) n'ajoute aucune entrée ; un refus (hors root, compte inconnu, sans option) n'est pas inscrit (pas de compte acteur).

Les deux codes appartiennent au type d'action « Droits sur les services » du filtre du client (`hearth-link::ActionKind::ServicesRights`) ; un test exige que tout code du catalogue appartienne à un type.

## Application (code)
- `crates/hearth-agent/src/application/services_right.rs::ServicesRightService::set`
- `crates/hearth-agent/src/domain/audit/action.rs::AuditAction::{AccountServicesGrant, AccountServicesRevoke}`
- `crates/hearth-proto/src/api/audit.rs::action::{ACCOUNT_SERVICES_GRANT, ACCOUNT_SERVICES_REVOKE}`
- `crates/hearth-link/src/domain/audit_query.rs::ActionKind::ServicesRights`

## Vérification
- `crates/hearth-agent/tests/services_grant_cli.rs::grant_then_revoke_change_the_right_say_so_and_are_journaled_from_the_command_line`
- `crates/hearth-agent/tests/services_grant_cli.rs::the_labels_of_the_journal_are_those_of_the_spec`
- `crates/hearth-agent/tests/services_grant_cli.rs::a_command_without_effect_says_so_and_adds_no_journal_entry`
- `crates/hearth-agent/tests/services_grant_cli.rs::the_entry_is_published_to_the_live_feed_only_after_it_is_written`
- `crates/hearth-agent/src/domain/audit/action.rs::tests::the_catalogue_of_the_protocol_lists_exactly_the_codes_of_the_agent`
- `crates/hearth-link/src/domain/audit_query.rs::tests::every_action_of_the_catalogue_belongs_to_a_kind_of_the_interface`
- `deploy/e2e/scenario.sh` (journal lu en base après octroi et retrait ; CI seulement)

## Cas limites
- L'entrée d'octroi de l'installation vient après celle de la création du compte (deux transactions) ; si l'octroi échoue, le compte est retiré.
- L'entrée du filtre « Droits sur les services » à l'écran du client (liste des types du filtre) relève du lot client de l'épique.

## Règles liées
- BR-GRANT-005, BR-GRANT-008

## Historique
- 2026-10-13 — création (HRT-77, ADR-0041).
