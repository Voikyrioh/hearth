---
id: BR-CONN-019
domaine: CONN
titre: Une adresse retenue d'un compte est l'un des trois critères de la règle « 2 sur 3 » ; elle n'est jamais un droit
statut: active
invariant: true
source: contexts/hearth/tickets/hrt/HRT-20.md (critères 1 et 2), ADR-0022, ADR-0024 ; Q15 ; contexts/hearth/tickets/hrt/HRT-24.md
maj: 2026-10-07
---

# BR-CONN-019 — Adresses connues d'un compte

## Règle
> **Règle créée par HRT-20 (ADR-0022), reprise par HRT-24 (ADR-0024)** : l'« adresse connue » devient l'« adresse retenue » (BR-TRUST-007), l'un des trois critères de BR-TRUST-001. Elle ne laisse plus passer à elle seule : voir « Effet ». Son **oubli au changement de son propre mot de passe** est un choix du titulaire (Q15, ci-dessous).

- **Acquisition** : une adresse devient connue d'un compte **uniquement** par une connexion **réussie** de ce compte depuis cette adresse.
- **Forme** : l'adresse **exacte** de la connexion TCP, canonique ; en IPv6 l'adresse complète, **jamais un préfixe** (aucun regroupement en /64).
- **Durée** : 30 jours après la dernière connexion réussie ; **nombre** : 8 par compte (la moins récente est oubliée).
- **Oubli** : sessions fermées par l'administration (BR-ACCT-011), mot de passe changé par l'administrateur ou par le titulaire (BR-ACCT-008, 009), compte supprimé (cascade). La simple déconnexion ne les oublie pas. **Depuis HRT-22** : une adresse liée à un poste à clé (BR-TRUST-007) survit au changement du **propre** mot de passe du titulaire (BR-TRUST-023) ; elle est oubliée avec son poste (BR-TRUST-022, 024).
- **Rafraîchissement par l'usage (HRT-22)** : une session valide utilisée depuis une adresse déjà retenue repousse sa durée (au plus toutes les 5 minutes) ; la durée de 30 jours se compte depuis le plus récent de la dernière connexion réussie et du dernier usage (`known_addresses.last_used_at`). L'usage d'une session n'**apprend** jamais une adresse.
- **Oubli au changement de son propre mot de passe (Q15, HRT-24)** : la requête porte un champ typé additif `keep_address`. Absent ou faux : comportement d'avant, **toutes** les adresses apprises sans clé sont oubliées, celle d'où part la requête comprise. Vrai : l'adresse d'où part la requête (la connexion TCP, jamais une valeur du corps) est **gardée** si elle est retenue, les autres oubliées. Les adresses liées à un poste à clé restent dans les deux cas (BR-TRUST-023). Rien n'est appris : garder une adresse qui n'était pas retenue n'en retient aucune.
- **Effet (HRT-24)** : en ALERTE, l'adresse retenue est un critère de la règle « 2 sur 3 » (BR-TRUST-001) : avec la clé, ou avec « du premier coup », le poste **passe malgré l'attente** du ralentissement par identifiant (BR-CONN-018). Elle ne suffit plus seule après une erreur de frappe (ADR-0024). Aucun autre compteur n'est levé : le couple et l'adresse s'appliquent à elle comme à toute autre (BR-CONN-006, 007).
- **Aucun droit** : le mot de passe reste exigé. Usurper une adresse connue (position d'interception sur le réseau local) donne au plus des essais comptés par le compteur du couple (5, puis 1 minute doublée, plafond 15 minutes).
- Un identifiant inexistant n'a aucune adresse connue ; la lecture est la même requête (jointure sur l'identifiant), zéro ligne (BR-CONN-013).
- Reprise à la migration : les adresses des sessions encore valides (au plus 8 par compte).

## Application (code)
- `crates/hearth-agent/src/domain/known_address.rs::{is_known, learn, cutoff, canonical, MAX_PER_ACCOUNT, VALIDITY}` ; `domain/login_policy.rs::conclude`.
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login_in_turn` (apprentissage à la réussite) ; `application/accounts.rs::{apply_password, revoke_sessions}` (oubli) ; `application/maintenance.rs::purge`.
- `crates/hearth-agent/src/infrastructure/sqlite/known_address_repo.rs` — table `known_addresses` (migration `0004`).

## Vérification
- Domaine : `domain::known_address::tests`, `domain::login_policy::tests`.
- Q15 : `tests/security_followups.rs::{keeping_the_address_keeps_it_and_the_key_poste_and_forgets_the_others, keeping_an_address_that_was_never_retained_learns_nothing, without_the_choice_every_address_without_a_key_is_forgotten_as_before, on_the_wire_the_field_is_additive_and_keeps_the_address_the_request_comes_from}` ; le cas par défaut garde ses tests d'avant (`tests/device_proof.rs::changing_your_own_password_keeps_the_devices_and_forgets_the_addresses_without_a_key`, `tests/login_lockout.rs::password_change_revocation_and_deletion_forget_the_known_addresses`).
- Intégration : `tests/login_lockout.rs::{the_admin_on_a_known_address_can_always_log_in_during_the_attack, ipv6_addresses_of_one_prefix_are_counted_one_by_one_never_as_a_prefix, failures_from_a_known_address_count_in_the_pair_and_the_address_but_not_in_the_identifier, a_spoofed_known_address_with_a_wrong_password_stays_locked_per_pair, a_known_address_with_a_wrong_password_never_opens_a_session, an_address_becomes_known_only_by_a_successful_login_of_that_account, at_most_eight_known_addresses_per_account_the_oldest_is_forgotten, a_known_address_is_forgotten_thirty_days_after_its_last_success, password_change_revocation_and_deletion_forget_the_known_addresses, a_plain_logout_keeps_the_known_address, the_migration_keeps_the_addresses_of_the_open_sessions_of_an_installed_database}`.

## Limites connues (modèle provisoire, écrites telles quelles)
- ~~Session de plus de 30 jours utilisée chaque jour : l'adresse n'était rafraîchie que par une connexion~~ : **levée par HRT-22** (rafraîchissement par l'usage, ci-dessus). La reprise de la migration 0004 prend toujours la date de **création** de la session, pas sa dernière activité.
- **Adresse IPv6 temporaire qui change** : la nouvelle adresse n'est connue qu'à la prochaine connexion réussie, même si la session ouverte reste valide.
- **Changement de son propre mot de passe** : sans la case « garder ce poste reconnu » (Q15), toutes les adresses sans clé sont oubliées, y compris celle de la session que le titulaire garde ; elle est rapprise à la prochaine connexion réussie.
- **Poste jamais vu** : voir BR-CONN-018 (ralenti, voire privé d'essais tant que l'attaque dure ; voies de secours : un poste connu, ou la commande `account` sur le serveur).

## Règles liées
- BR-CONN-006, BR-CONN-007, BR-CONN-013, BR-CONN-018, BR-CONN-020, BR-RESIL-012.

## Historique
- 2026-10-06 — création (HRT-20, ADR-0022). Ajustée après la review de la PR 23 : seul effet = passer malgré le ralentissement (plus d'exemption du compteur par adresse), pas de /64, marquée provisoire.
- 2026-10-07 : HRT-24 (T33) : critère de la règle « 2 sur 3 » à la place de l'exception « adresse connue » ; champ `keep_address` (Q15, amendement nommé du ticket).
- 2026-10-07 : HRT-22, table étendue (migration 0005 : device_id, last_used_at), adresse liée à un poste à clé, rafraîchie par l'usage, apprise aussi par session + preuve de clé (BR-TRUST-007, ADR-0023). Énoncé de l'acquisition par connexion réussie inchangé.
- 2026-10-07 : la case « Garder ce poste reconnu » de la fenêtre de changement de son mot de passe (HRT-26, T38, Q15) : DÉCOCHÉE par défaut (le choix le plus strict, à confirmer), envoie `keep_address` seulement quand elle est cochée ; désactivée avec sa raison quand l'agent est trop ancien pour la connaître ; si le mode attaque est actif sur un poste sans clé enregistrée, un avertissement dit que l'oublier refuse la session tout de suite (adresse oubliée : la session devient « seule », BR-TRUST-013). `apps/desktop/src-tauri/src/accounts/wire.rs::change_own_password`, `components/organisms/PasswordDialog.vue` ; tests `apps/desktop/src-tauri/tests/accounts_wire.rs::keeping_this_pc_recognized_is_sent_only_when_the_box_is_ticked`, `src/pages/SecuritySettings.test.ts`.
