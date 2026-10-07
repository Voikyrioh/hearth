---
id: BR-TRUST-008
domaine: TRUST
titre: Quand l'identifiant d'un utilisateur est visé par des tentatives refusées venues d'autres appareils, il reçoit une alerte de probable attaque
statut: partielle
invariant: false
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-008) ; conception technique 2026-10-06 (5.2, 5.7, 8.4) ; contexts/hearth/tickets/hrt/HRT-24.md ; ADR-0024
maj: 2026-10-07
---

# BR-TRUST-008 : Alerte quand mon identifiant est visé

## Règle
- **L'alerte se déduit du compteur par identifiant** (BR-CONN-018), jamais d'une table d'état : plus de 10 échecs venus d'adresses inconnues de tous les comptes et un dernier échec de moins de 30 minutes, ou une attente en cours. Elle commence au moment exact où l'identifiant commence à être ralenti et s'éteint après 30 minutes sans échec. Les erreurs de frappe du titulaire depuis un poste dont l'adresse est retenue ne nourrissent pas ce compteur : pas d'alerte pour ses propres erreurs.
- **Une fois par épisode** : la colonne `identifier_slowdowns.alerted_at` note le début. L'écriture se fait dans une tâche détachée (même lancement pour un identifiant existant ou non). La transition « début » consigne une entrée `security.alert` (cible « début de l'alerte », compte visé, origine = la tentative qui l'a ouverte) et prévient le flux ; la transition « fin » (30 minutes sans échec, par la tâche périodique toutes les 30 s, ou par le compteur qui repart de zéro) consigne `security.alert` « fin de l'alerte (levée par l'agent) » (origine sans adresse ; une origine système demanderait une migration, à grouper avec HRT-25 ; quand la fin vient d'un compteur qui repart de zéro, l'origine est l'adresse de l'appareil qui relance l'attaque) et prévient le flux. Pas de notification à chaque tentative.
- **À qui** : le titulaire de l'identifiant visé, quel que soit son rôle (`alert.own`, `alert.since`) ; un **administrateur** voit en plus **combien** d'autres comptes existants sont visés (`alert.others`), jamais leurs noms ; un autre compte n'apprend jamais qu'un autre identifiant est visé (`others` absent). Un identifiant **inexistant** ne déclenche **aucune** alerte, ni entrée de journal, ni message (l'épisode est noté pareil dans la table, par la même écriture, pour qu'aucune différence ne soit observable).
- **Canaux** : `GET /api/v1/security` (tout rôle) ; message de flux `security`, toujours envoyé après l'`auth` (sans abonnement), puis à chaque changement de l'état de CE compte.

> **Partielle** : la notification Windows, le bandeau et son bouton viennent avec HRT-26 (client). Le mode attaque du message (`attack_mode`) vaut toujours `off` jusqu'à HRT-25.

## Application (code)
- `crates/hearth-agent/src/domain/identifier_slowdown.rs::{is_alert, ended, alert_change, AlertChange, Slowdown::alerted_at}`.
- `crates/hearth-agent/src/application/security.rs::SecurityService::{state_for, alert_started, alert_ended, sweep}` ; `application/sessions.rs::SessionService::signal_alert`.
- `crates/hearth-agent/src/entrypoint/http/security.rs` ; `entrypoint/ws/connection.rs::push_security` ; `infrastructure/security_feed.rs` ; `entrypoint/tasks.rs::spawn_alert_sweep`.
- `crates/hearth-proto/src/api/security.rs`, `stream.rs::SecurityMessage`.

## Vérification
- `tests/security_alert.rs::{the_alert_is_signalled_once_when_the_identifier_starts_to_be_slowed_and_never_for_a_missing_one, the_end_of_the_alert_is_signalled_once_by_the_sweep_and_a_new_episode_starts_clean, a_counter_that_starts_over_without_a_sweep_still_ends_the_old_episode_once, my_own_typos_from_my_recognised_poste_never_raise_an_alert, the_owner_sees_the_alert_an_administrator_sees_the_count_of_the_others_and_a_readonly_only_its_own, the_stream_always_tells_the_state_after_auth_then_on_every_change_and_only_what_the_role_shows, the_security_route_needs_a_session}`.
- Horloge et purge : `tests/security_alert.rs::{a_wall_clock_set_back_or_forward_never_locks_anybody_out_and_ends_the_alert_once, the_purge_keeps_an_alert_in_progress_and_forgets_an_ended_one_with_its_mark, the_eleventh_answer_is_the_same_for_a_missing_and_an_existing_identifier_and_writes_nothing_itself}`.
- Domaine : `domain::identifier_slowdown::tests::{the_alert_starts_with_the_first_wait_and_is_noted_once_per_episode, a_counter_that_starts_over_ends_the_episode, an_episode_is_ended_for_the_sweep_when_its_conditions_no_longer_hold, ten_failures_are_not_an_alert_the_eleventh_is}`.

## Règles liées
- BR-CONN-018, BR-TRUST-006, BR-AUDIT-006, ADR-0024.

## Historique
- 2026-10-07 : création (HRT-24, session 2026-10-04-hearth-creation, T33).
