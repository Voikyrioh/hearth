---
id: BR-TRUST-025
domaine: TRUST
titre: Quand un compte est supprimé, toutes les clés de tous ses postes reconnus sont effacées du serveur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md ; conception technique 2026-10-06 ; contexts/hearth/tickets/hrt/HRT-22.md ; ADR-0023
maj: 2026-10-07
---

# BR-TRUST-025 : Quand un compte est supprimé, toutes les clés de tous ses postes reconnus sont effacées du serveur

## Règle
Supprimer un compte efface du serveur, dans la même transaction, tous ses postes de confiance (donc toutes ses clés publiques) et toutes ses adresses retenues : cascade de la clé étrangère `trusted_devices.account_id` et `known_addresses.account_id`. Les postes des autres comptes ne sont pas touchés.

## Application (code)
- `crates/hearth-agent/migrations/0005_trusted_devices_attack_mode.sql` (`ON DELETE CASCADE`) ; `application/accounts.rs::AccountService::delete`.

## Vérification
- `tests/device_proof.rs::deleting_the_account_erases_its_devices_and_addresses_and_only_its_own`.
- `tests/migration_0005.rs::the_constraints_of_the_new_tables_hold` (cascade du compte, du poste, détachement de la session).

## Cas limites
- La clé privée n'est jamais sur le serveur : il n'y a que la clé publique à effacer.

## Règles liées
- BR-TRUST-004, BR-ACCT-010, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
