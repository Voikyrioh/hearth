---
id: BR-TRUST-023
domaine: TRUST
titre: Quand un utilisateur change son propre mot de passe, la clé de chacun de ses postes reconnus survit et reste valide
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md ; conception technique 2026-10-06 ; contexts/hearth/tickets/hrt/HRT-22.md ; ADR-0023
maj: 2026-10-07
---

# BR-TRUST-023 : Quand un utilisateur change son propre mot de passe, la clé de chacun de ses postes reconnus survit et reste valide

## Règle
Quand le titulaire change **son propre** mot de passe, la clé de chacun de ses postes reconnus survit et reste valide (les postes restent inscrits, avec l'adresse retenue qui leur est liée). Ses autres sessions sont fermées (BR-ACCT-009) ; les autres postes prouveront leur clé à leur reconnexion par mot de passe. Les adresses apprises **sans** clé sont oubliées (ADR-0022, BR-CONN-019 : inchangé).

## Application (code)
- `crates/hearth-agent/src/application/accounts.rs::AccountService::apply_password` (branche « titulaire » : `known_addresses().forget_without_device`) ; `infrastructure/sqlite/known_address_repo.rs::forget_without_device`.

## Vérification
- `tests/device_proof.rs::changing_your_own_password_keeps_the_devices_and_forgets_the_addresses_without_a_key`.
- `tests/login_lockout.rs::password_change_revocation_and_deletion_forget_the_known_addresses` (sans poste à clé : toutes les adresses sont oubliées, comme avant).

## Cas limites
- La conception propose de garder, en plus, l'adresse d'où part la requête (« à confirmer »). **Non retenu** : elle contredit BR-CONN-019 et son test (changement de son propre mot de passe : toutes les adresses sont oubliées) et n'est pas confirmée par le détenteur.

## Règles liées
- BR-TRUST-024, BR-ACCT-008, BR-ACCT-009, BR-CONN-019, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
