---
id: BR-CONN-019
domaine: CONN
titre: Une adresse connue d'un compte évite d'être ralentie par les échecs des autres, mais ne donne aucun droit
statut: active
invariant: true
source: contexts/hearth/tickets/hrt/HRT-20.md (critères 1 et 2), ADR-0022
maj: 2026-10-06
---

# BR-CONN-019 — Adresses connues d'un compte

## Règle
> **Règle nouvelle**, absente de la spécification fonctionnelle : créée par le ticket HRT-20 et l'ADR-0022 (source ci-dessus).

- **Acquisition** : une adresse devient connue d'un compte **uniquement** par une connexion **réussie** de ce compte depuis cette adresse (mot de passe vérifié, dans la transaction qui ouvre la session). Ni un échec, ni une session d'un autre compte, ni la lecture d'un journal ne l'apprennent.
- **Forme** : l'adresse **exacte** de la connexion TCP, en forme canonique (IPv4 reçue sur socket double pile ramenée à IPv4). En IPv6, l'adresse complète (/128), **jamais** le préfixe : sinon tout appareil du foyer, qui partage le /64, serait « connu ».
- **Durée** : 30 jours après la dernière connexion réussie depuis cette adresse ; chaque connexion réussie la rafraîchit. **Nombre** : 8 par compte, la moins récemment réussie est oubliée au-delà.
- **Oubli** : sessions fermées par l'administration (BR-ACCT-011), mot de passe changé par l'administrateur ou par le titulaire (BR-ACCT-008, 009) : toutes les adresses connues du compte, dans la même transaction. Compte supprimé : suppression en cascade. La simple déconnexion ne les oublie pas.
- **Effet** : sur une adresse connue **du compte visé**, le ralentissement par identifiant (BR-CONN-018) et le compteur par origine (BR-CONN-007) ne s'appliquent pas, et ses échecs ne les nourrissent pas (une adresse usurpée ne gêne personne d'autre). Elle occupe aussi les places réservées des connexions en cours (BR-CONN-020).
- **Aucun droit** : une adresse connue ne dispense **jamais** du mot de passe, et le compteur du **couple** identifiant + adresse reste en place pour elle (BR-CONN-006 : 5 échecs, puis 1 minute doublée, plafond 15 minutes). Usurper une adresse connue (possible seulement en position d'interception sur le réseau local) donne au plus des essais comptés par ce couple.
- **Identifiant inexistant** : aucune adresse n'est connue (zéro ligne par la même requête que pour un identifiant existant, BR-CONN-013).
- Reprise à la migration : les adresses des sessions encore valides deviennent connues (au plus 8 par compte).

## Application (code)
- `crates/hearth-agent/src/domain/known_address.rs::{is_known, learn, cutoff, MAX_PER_ACCOUNT, VALIDITY}` ; `domain/login_origin.rs::canonical` ; `domain/login_policy.rs::{admit, after_failure}`.
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login_in_turn` (apprentissage à la réussite) ; `application/accounts.rs::{apply_password, revoke_sessions}` (oubli) ; `application/maintenance.rs::purge`.
- `crates/hearth-agent/src/infrastructure/sqlite/login_attempt_repo.rs` — table `known_addresses` (migration `0004`).

## Vérification
- Domaine : `domain::known_address::tests`, `domain::login_origin::tests`, `domain::login_policy::tests`.
- Intégration : `tests/login_lockout.rs::{the_admin_on_a_known_address_can_always_log_in_during_the_attack, a_known_address_is_not_blocked_by_the_origin_counter_of_its_ipv6_prefix, a_known_address_never_feeds_the_origin_or_the_identifier_counters, a_spoofed_known_address_with_a_wrong_password_stays_locked_per_pair, a_known_address_with_a_wrong_password_never_opens_a_session, an_address_becomes_known_only_by_a_successful_login_of_that_account, at_most_eight_known_addresses_per_account_the_oldest_is_forgotten, a_known_address_is_forgotten_thirty_days_after_its_last_success, password_change_revocation_and_deletion_forget_the_known_addresses, a_plain_logout_keeps_the_known_address, the_migration_keeps_the_addresses_of_the_open_sessions_of_an_installed_database}`.

## Cas limites
- Une adresse IPv6 temporaire qui change (privacy extensions) cesse d'être connue : la session déjà ouverte n'a pas besoin de connexion, et la nouvelle adresse est apprise à la prochaine connexion réussie.
- Après un changement de mot de passe, la prochaine connexion réussie rapprend l'adresse.

## Règles liées
- BR-CONN-006, BR-CONN-007, BR-CONN-013, BR-CONN-018, BR-CONN-020, BR-RESIL-012.

## Historique
- 2026-10-06 — création (HRT-20, ADR-0022).
