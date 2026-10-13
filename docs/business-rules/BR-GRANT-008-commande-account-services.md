---
id: BR-GRANT-008
domaine: GRANT
titre: hearth-agent account services {compte} --grant ou --revoke : root seulement, compte existant, une option, sans effet si l'état est déjà atteint
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-008), contexts/hearth/conceptions/2026-10-13-adr-droit-de-gerer-les-services-brouillon.md, HRT-77
maj: 2026-10-13
---

# BR-GRANT-008 — hearth-agent account services {compte} --grant ou --revoke : root seulement, compte existant, une option, sans effet si l'état est déjà atteint

## Règle
La commande accorde (`--grant`) ou retire (`--revoke`) le droit à un compte existant. Ordre des contrôles : root (« Cette commande doit être lancée en root sur le serveur. », avant de lire la base et avant de créer le dossier de données ; l'utilisateur effectif est lu sur la machine, jamais sur une option ou une variable), puis l'option (« Précise --grant ou --revoke. »), puis le compte (« Aucun compte « {compte} » sur ce serveur. »). Les deux options ensemble sont refusées à l'analyse de la ligne. Aucune autre option (pas de mot de passe, pas de rôle).

Compte déjà dans l'état demandé : « Le compte {compte} peut déjà gérer les services de ce serveur. » ou « Le compte {compte} ne pouvait pas gérer les services de ce serveur. » ; rien n'est écrit, AUCUNE entrée de journal. Succès : « Le compte {compte} peut maintenant gérer les services de ce serveur. » ou « Le compte {compte} ne peut plus gérer les services de ce serveur. », dit APRÈS la validation commune du changement et de son entrée de journal.

Le nom passé en argument est un identifiant (3 à 32 caractères : minuscules, chiffres, tiret, underscore) : il est comparé à la base par une requête paramétrée, jamais interprété ni exécuté ; un nom qui n'est pas un identifiant valide ne désigne aucun compte, et il est rejoué dans le message sans caractère de contrôle (remplacé par « ? », 64 caractères au plus). C'est, avec l'octroi de l'installation, le SEUL chemin qui écrit la colonne : le cas d'usage `ServicesRightService` n'est dans aucun état remis au serveur HTTP (BR-GRANT-005).

## Application (code)
- `crates/hearth-agent/src/application/services_right.rs::ServicesRightService::{existing, set}`
- `crates/hearth-agent/src/entrypoint/account.rs::{execute, change_services_right, require_root}`
- `crates/hearth-agent/src/app.rs::{run, services_right_service}` (contrôle de root avant tout)
- `crates/hearth-agent/src/infrastructure/sqlite/services_right_repo.rs` (l'unique requête d'écriture)

## Vérification
- `crates/hearth-agent/tests/services_grant_cli.rs` (octroi, retrait, sans effet, compte inconnu, sans option, hors root, nom piégé, compte créé sans le droit)
- `crates/hearth-agent/tests/account_cli.rs::the_services_command_refuses_outside_root_before_touching_anything`
- `crates/hearth-agent/tests/account_cli.rs::the_services_command_takes_exactly_one_option_and_no_password`
- `crates/hearth-agent/src/entrypoint/cli.rs::tests::the_services_command_takes_one_account_and_at_most_one_option`
- `crates/hearth-agent/src/entrypoint/account.rs::tests` (messages mot pour mot, nom jamais rejoué avec des caractères de contrôle)
- `crates/hearth-agent/tests/services_right_never_written.rs::only_the_command_line_and_the_installation_can_reach_the_write`
- `deploy/e2e/scenario.sh` (section « Droit de gérer les services », CI seulement)

## Cas limites
- Deux commandes en même temps : la dernière écrite fait foi ; chacune journalise ce qu'elle a changé (relecture dans la transaction).
- Retirer le droit ne ferme aucune session et ne change pas le rôle (BR-GRANT-010).
- Le dernier titulaire peut perdre le droit : aucun minimum (BR-GRANT-012).

## Règles liées
- BR-GRANT-005, BR-GRANT-009, BR-GRANT-010, BR-GRANT-012

## Historique
- 2026-10-13 — création (HRT-77, ADR-0041).
