---
id: BR-GRANT-002
domaine: GRANT
titre: Le droit n'est accordé à personne à l'arrivée de la migration, et un compte créé naît sans le droit, quel que soit son rôle
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-002), contexts/hearth/conceptions/2026-10-13-adr-droit-de-gerer-les-services-brouillon.md, HRT-76
maj: 2026-10-13
---

# BR-GRANT-002 — Le droit n'est accordé à personne à l'arrivée de la migration, et un compte créé naît sans le droit, quel que soit son rôle

## Règle
La migration `0010` ajoute la colonne `accounts.services_right TEXT NOT NULL DEFAULT 'none' CHECK (services_right IN ('none','manage'))` : tous les comptes existants (premier administrateur compris) la prennent à `none`, rien n'est accordé en silence. Un compte créé par la route `POST /accounts` naît sans le droit, que son rôle soit administrateur ou lecture seule et quoi que le corps de la requête contienne (`rights`, `services_right` : champs jamais lus). La requête d'insertion de l'agent ne nomme pas la colonne : le défaut de la base fait foi (`ServicesRight::INITIAL`). Un compte créé en ligne de commande sur le serveur (`account add`) naît aussi sans le droit, sans aucune option pour le donner à la création : le droit s'accorde ensuite par `account services` (BR-GRANT-008). L'installation ne le donne qu'au PREMIER compte, sur question ou option explicite (BR-GRANT-006, BR-GRANT-007) ; une mise à jour de l'agent n'accorde rien aux comptes existants et son guide donne la commande.

## Application (code)
- `crates/hearth-agent/migrations/0010_services_right.sql`
- `crates/hearth-agent/src/domain/accounts/rights.rs::ServicesRight::INITIAL`
- `crates/hearth-agent/src/application/accounts.rs::AccountService::insert_new`
- `crates/hearth-agent/src/infrastructure/sqlite/account_repo.rs::SqliteUnitOfWork::insert`

## Vérification
- `crates/hearth-agent/tests/migration_0010.rs::no_existing_account_gets_the_right_not_even_the_first_administrator`
- `crates/hearth-agent/tests/migration_0010.rs::an_account_inserted_without_naming_the_column_is_born_without_the_right`
- `crates/hearth-agent/tests/migration_0010.rs::the_column_only_holds_the_two_known_values`
- `crates/hearth-agent/tests/migration_0010.rs::running_all_migrations_twice_changes_nothing`
- `crates/hearth-agent/tests/http_api.rs::a_created_account_is_born_without_the_right_and_rights_are_described_on_the_wire`
- `crates/hearth-agent/tests/services_grant_cli.rs::an_account_created_on_the_command_line_is_born_without_the_right`
- `crates/hearth-agent/tests/install_flow.rs::an_update_never_asks_nor_grants_to_the_accounts_that_already_exist`
- `crates/hearth-agent/src/domain/accounts/rights.rs::tests::a_new_account_has_no_right`

## Cas limites
- Une valeur inconnue en base (colonne altérée à la main, `CHECK` contourné) est refusée à la lecture (`corrupt("droit sur les services")`) : jamais lue comme un droit accordé.
- Rejouer les migrations ne retire ni n'accorde un droit déjà posé.

## Règles liées
- BR-GRANT-001, BR-GRANT-005, BR-GRANT-006, BR-GRANT-008

## Historique
- 2026-10-13 — création (HRT-76, ADR-0041 : le droit de gérer les services, moitié agent).
- 2026-10-13 — HRT-77 : compte créé en ligne de commande, installation et mise à jour.
