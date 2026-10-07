---
id: BR-TRUST-001
domaine: TRUST
titre: Un poste est reconnu s'il présente au moins deux des trois critères : adresse retenue, session valide ou authentification du premier coup, clé de l'appareil
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-001) ; conception technique 2026-10-06 (5.3, 10) ; Q10, Q11, Q12, Q13, Q14 (point 5) ; contexts/hearth/tickets/hrt/HRT-24.md, HRT-25.md ; ADR-0024, ADR-0025
maj: 2026-10-07
---

# BR-TRUST-001 : Reconnaissance d'un poste (règle « 2 critères sur 3 »)

## Règle
Un poste est **reconnu** s'il présente au moins deux de ces trois critères :

- (a) l'**adresse retenue** pour le compte visé (BR-TRUST-007) ;
- (b) une **session valide**, OU une **authentification réussie du premier coup** sur le bon compte : le mot de passe est juste et le compteur du couple (identifiant, adresse) est à zéro, aucun échec compté depuis son dernier succès (Q14, point 5) ;
- (c) la **preuve** de la bonne clé de l'appareil : clé inscrite pour le compte visé (BR-TRUST-005).

Une connexion par mot de passe ne présente jamais de session : une session n'apporte rien de plus qu'un mot de passe juste. Pour elle, le critère (b) est « du premier coup ». **La règle ne décide que deux choses** : ce poste échappe-t-il au ralentissement par identifiant, et le mot de passe est-il pris en compte. Elle ne dit jamais « accordé » (BR-TRUST-002).

Table de vérité d'une connexion en **ALERTE** (BR-TRUST-006 ; adresse, clé, premier coup) :

| adresse | clé | premier coup | échappe au ralentissement |
|---|---|---|---|
| oui | oui | oui ou non | oui |
| oui | non | oui | oui |
| non | oui | oui | oui |
| oui | non | non | non |
| non | oui | non | non |
| non | non | oui ou non | non |

En **NORMAL** la règle ne joue pas (BR-TRUST-035) : personne n'échappe à rien, parce que rien ne ralentit.

En **MODE ATTAQUE** (HRT-25, BR-TRUST-011) la même fonction décide qui passe et qui est bloqué : deux critères avant le mot de passe, reconnu ; un seul, UN essai (BR-TRUST-012) ; aucun, bloqué sans essai (BR-TRUST-016). L'usage d'une session valide est jugé par `judge_session` : une session seule est refusée en mode attaque (BR-TRUST-013), elle fonctionne en NORMAL et en ALERTE.

## Application (code)
- `crates/hearth-agent/src/domain/trust/recognition.rs::{judge_login, judge_session, mode_of, Mode, LoginCriteria, LoginStanding, SessionStanding}` (fonctions pures).
- `crates/hearth-agent/src/application/sessions.rs::SessionService::verify` : lit toutes les entrées de la règle pour tout identifiant (liste d'adresses vide et clé jamais reconnue pour un compte absent), appelle `judge_login`, puis `login_policy::conclude` (`escapes_slowdown` remplace le `known` provisoire de l'ADR-0022).
- `crates/hearth-agent/src/domain/login_policy.rs::conclude`.

## Vérification
- Domaine : `domain::trust::recognition::tests::{the_whole_truth_table_in_alert_is_the_one_written_by_hand, in_the_normal_state_the_rule_changes_nothing_whatever_the_criteria, the_rule_never_discards_the_password_in_the_states_of_this_ticket, the_first_try_alone_or_one_criterion_alone_is_never_enough, the_mode_is_derived_from_the_counter_and_starts_at_the_eleventh_failure, the_alert_ends_after_thirty_minutes_without_failure, a_wall_clock_set_back_or_forward_never_blocks_nor_freezes_the_state}`.
- Intégration : `tests/security_alert.rs::{in_alert_every_combination_of_criteria_is_recognised_as_the_table_says_and_nobody_is_locked_out, in_the_normal_state_the_rule_changes_nothing_a_stranger_connects_like_a_known_poste, a_recognised_poste_is_not_slowed_while_other_devices_pile_up_failures, my_pc_changes_address_and_stays_recognised_thanks_to_the_key_and_the_new_address_is_retained, nothing_a_deviceless_appliance_does_ever_refuses_a_poste_with_two_criteria_and_the_right_password}`.

## Cas limites
- Un poste qui s'est trompé de mot de passe depuis son poste n'est plus « du premier coup » : en alerte son adresse seule ne suffit plus, il est ralenti (jamais bloqué) et le bon mot de passe passe dès la fin de l'attente (au plus 2 minutes). Changement par rapport à l'ADR-0022 (« adresse connue ») : `tests/login_review.rs::a_slowed_identifier_answers_a_known_address_like_a_missing_one_unless_the_password_is_right`.
- La preuve d'une clé non inscrite pour le compte visé ne vaut pas le critère (c).

## Règles liées
- BR-TRUST-002, BR-TRUST-005, BR-TRUST-006, BR-TRUST-007, BR-TRUST-011, BR-TRUST-012, BR-TRUST-013, BR-TRUST-034, BR-TRUST-035, BR-CONN-018, BR-CONN-019, ADR-0024, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-24, session 2026-10-04-hearth-creation, T33).
- 2026-10-07 : HRT-25, le mode attaque (`Mode::Attack`, essai unique, `judge_session`) complète la règle (T34).
