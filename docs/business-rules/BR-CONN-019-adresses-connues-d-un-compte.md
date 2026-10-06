---
id: BR-CONN-019
domaine: CONN
titre: Une adresse connue d'un compte laisse son titulaire passer malgré le ralentissement, sans lui donner aucun droit (provisoire)
statut: active
invariant: true
source: contexts/hearth/tickets/hrt/HRT-20.md (critères 1 et 2), ADR-0022
maj: 2026-10-06
---

# BR-CONN-019 — Adresses connues d'un compte

## Règle
> **Règle nouvelle**, absente de la spécification fonctionnelle : créée par le ticket HRT-20 et l'ADR-0022. **PROVISOIRE : sera remplacée par l'identité d'appareil par clé** (échange de clé à la connexion ; conception à venir). Livrée telle quelle, avec ses limites connues ci-dessous.

- **Acquisition** : une adresse devient connue d'un compte **uniquement** par une connexion **réussie** de ce compte depuis cette adresse.
- **Forme** : l'adresse **exacte** de la connexion TCP, canonique ; en IPv6 l'adresse complète, **jamais un préfixe** (aucun regroupement en /64).
- **Durée** : 30 jours après la dernière connexion réussie ; **nombre** : 8 par compte (la moins récente est oubliée).
- **Oubli** : sessions fermées par l'administration (BR-ACCT-011), mot de passe changé par l'administrateur ou par le titulaire (BR-ACCT-008, 009), compte supprimé (cascade). La simple déconnexion ne les oublie pas.
- **Effet, et seul effet** : le titulaire du mot de passe, depuis une adresse connue de son compte, **passe malgré l'attente** du ralentissement par identifiant (BR-CONN-018). Aucun autre compteur n'est levé : le couple et l'adresse s'appliquent à elle comme à toute autre (BR-CONN-006, 007).
- **Aucun droit** : le mot de passe reste exigé. Usurper une adresse connue (position d'interception sur le réseau local) donne au plus des essais comptés par le compteur du couple (5, puis 1 minute doublée, plafond 15 minutes).
- Un identifiant inexistant n'a aucune adresse connue ; la lecture est la même requête (jointure sur l'identifiant), zéro ligne (BR-CONN-013).
- Reprise à la migration : les adresses des sessions encore valides (au plus 8 par compte).

## Application (code)
- `crates/hearth-agent/src/domain/known_address.rs::{is_known, learn, cutoff, canonical, MAX_PER_ACCOUNT, VALIDITY}` ; `domain/login_policy.rs::conclude`.
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login_in_turn` (apprentissage à la réussite) ; `application/accounts.rs::{apply_password, revoke_sessions}` (oubli) ; `application/maintenance.rs::purge`.
- `crates/hearth-agent/src/infrastructure/sqlite/known_address_repo.rs` — table `known_addresses` (migration `0004`).

## Vérification
- Domaine : `domain::known_address::tests`, `domain::login_policy::tests`.
- Intégration : `tests/login_lockout.rs::{the_admin_on_a_known_address_can_always_log_in_during_the_attack, ipv6_addresses_of_one_prefix_are_counted_one_by_one_never_as_a_prefix, failures_from_a_known_address_count_in_the_pair_and_the_address_but_not_in_the_identifier, a_spoofed_known_address_with_a_wrong_password_stays_locked_per_pair, a_known_address_with_a_wrong_password_never_opens_a_session, an_address_becomes_known_only_by_a_successful_login_of_that_account, at_most_eight_known_addresses_per_account_the_oldest_is_forgotten, a_known_address_is_forgotten_thirty_days_after_its_last_success, password_change_revocation_and_deletion_forget_the_known_addresses, a_plain_logout_keeps_the_known_address, the_migration_keeps_the_addresses_of_the_open_sessions_of_an_installed_database}`.

## Limites connues (modèle provisoire, écrites telles quelles)
- **Session de plus de 30 jours** utilisée chaque jour : l'adresse n'est rafraîchie que par une connexion, jamais par l'usage de la session ; passé 30 jours sans nouvelle connexion, le poste n'est plus connu. La reprise de la migration prend la date de **création** de la session, pas sa dernière activité.
- **Adresse IPv6 temporaire qui change** : la nouvelle adresse n'est connue qu'à la prochaine connexion réussie, même si la session ouverte reste valide.
- **Changement de son propre mot de passe** : toutes les adresses du compte sont oubliées, y compris celle de la session que le titulaire garde ; elle est rapprise à la prochaine connexion réussie.
- **Poste jamais vu** : voir BR-CONN-018 (ralenti, voire privé d'essais tant que l'attaque dure ; voies de secours : un poste connu, ou la commande `account` sur le serveur).

## Règles liées
- BR-CONN-006, BR-CONN-007, BR-CONN-013, BR-CONN-018, BR-CONN-020, BR-RESIL-012.

## Historique
- 2026-10-06 — création (HRT-20, ADR-0022). Ajustée après la review de la PR 23 : seul effet = passer malgré le ralentissement (plus d'exemption du compteur par adresse), pas de /64, marquée provisoire.
