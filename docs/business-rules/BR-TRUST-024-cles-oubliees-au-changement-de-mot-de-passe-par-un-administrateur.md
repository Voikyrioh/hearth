---
id: BR-TRUST-024
domaine: TRUST
titre: Quand un administrateur change le mot de passe d'un autre compte, ou ferme ses sessions, tous les postes reconnus de ce compte sont oubliés
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md ; conception technique 2026-10-06 ; contexts/hearth/tickets/hrt/HRT-22.md ; ADR-0023
maj: 2026-10-07
---

# BR-TRUST-024 : Quand un administrateur change le mot de passe d'un autre compte, ou ferme ses sessions, tous les postes reconnus de ce compte sont oubliés

## Règle
Quand un administrateur change le mot de passe d'un autre compte (BR-ACCT-008), tous les postes reconnus de ce compte **et toutes ses adresses retenues** sont oubliés, dans la même transaction que le changement et la fermeture des sessions. Il en va de même quand un administrateur ferme les sessions du compte (BR-ACCT-011, route ou ligne de commande) : « fermer les sessions » est le geste pour un poste volé. La même clé du coffre est réinscrite à la reconnexion suivante avec le nouveau mot de passe (`enrolled`).

## Application (code)
- `crates/hearth-agent/src/application/accounts.rs::AccountService::{apply_password, revoke_sessions}` (`devices().forget_all` puis `known_addresses().forget`) ; `infrastructure/sqlite/device_repo.rs::forget_all`.

## Vérification
- `tests/device_proof.rs` : `a_password_change_by_an_administrator_forgets_every_device_and_address`, `closing_the_sessions_forgets_the_devices_and_the_addresses_of_the_account`.

## Cas limites
- Fermer les sessions oublie aussi les postes : choix de conception (section 18, point 11), non demandé au détenteur.

## Règles liées
- BR-TRUST-022, BR-TRUST-023, BR-ACCT-008, BR-ACCT-011, BR-CONN-019, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
