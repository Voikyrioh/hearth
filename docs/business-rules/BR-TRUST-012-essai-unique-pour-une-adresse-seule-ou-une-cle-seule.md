---
id: BR-TRUST-012
domaine: TRUST
titre: En mode attaque, un poste qui ne présente qu'un critère autre que la session (adresse seule, clé seule) a droit à UN essai de mot de passe
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-012) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-012 : En mode attaque, un poste qui ne présente qu'un critère autre que la session (adresse seule, clé seule) a droit à UN essai de mot de passe

## Règle
Un essai, **par critère présenté, par compte et par activation** du mode attaque : clé primaire `(activation_id, compte, sorte, sujet)` de `attack_trials`, où la sorte est `address` (sujet : l'adresse exacte, forme canonique) ou `key` (sujet : l'identifiant du poste). Au plus 16 lignes par compte et par activation (8 adresses, 8 postes).

- Seul un essai **raté** consomme l'essai (BR-TRUST-015) ; un essai **réussi** laisse le poste reconnu pour l'activation (BR-TRUST-014) et peut se reconnecter. Un mot de passe faux ensuite depuis ce critère fait passer la ligne de réussie à ratée. L'issue est écrite dans la transaction de la tentative (la même que les compteurs), jamais pour une tentative refusée avant d'avoir été comptée. La lecture et l'écriture sont dans la même transaction `BEGIN IMMEDIATE` : deux tentatives simultanées sur le même essai ne donnent pas deux échecs gratuits (la seconde le trouve déjà raté).
- Un attaquant qui usurpe les adresses retenues d'un compte obtient **au plus 8 essais par activation** (une par adresse retenue), **0** sans usurpation. Il n'a pas de clé inscrite : les essais de clé ne lui profitent pas.
- **Remise à zéro** : chaque activation distincte a un nouvel `activation_id` (ULID) et supprime les essais précédents. La reprise après suspension (BR-TRUST-020) garde le même identifiant.
- **Garde de réactivation** (Q14 point 8) : une activation faite moins de 30 minutes après la fin de la précédente la **prolonge** (même `activation_id`, essais non rendus). Elle se mesure sur le temps écoulé depuis le démarrage du noyau tant que la machine n'a pas redémarré, sur l'horloge murale ensuite (une fin « dans le futur » compte comme récente).
- Un essai réussi est « du premier coup » (BR-TRUST-014) ; raté, il bloque (BR-TRUST-015).

## Application (code)
- `crates/hearth-agent/src/domain/trust/recognition.rs::judge_login` (`LoginStanding::trial`).
- `crates/hearth-agent/src/application/ports/attack_mode_repo.rs::AttackModeTx::{trial_used, record_trial}` et `crates/hearth-agent/src/infrastructure/sqlite/attack_mode_repo.rs`.
- `crates/hearth-agent/src/domain/trust/attack_mode.rs::plan_activation` (garde), `crates/hearth-agent/src/application/attack_mode.rs::AttackModeService::change`.

## Vérification
- `attack_mode.rs` : `a_key_alone_after_an_address_change_has_exactly_one_trial`, `an_address_alone_without_key_has_exactly_one_trial`, `an_attacker_spoofing_the_eight_retained_addresses_obtains_exactly_eight_trials_and_not_one_more`, `without_spoofing_an_attacker_obtains_zero_trial`, `rapid_activation_cycles_give_no_trial_back_until_thirty_minutes_after_the_end`, `the_reactivation_guard_does_not_depend_on_the_wall_clock_on_the_same_boot`, `two_simultaneous_attempts_on_the_same_trial_never_give_two_free_failures` (plusieurs fils, concurrence réelle).
- Domaine : `domain::trust::attack_mode::tests::{a_reactivation_under_thirty_minutes_prolongs_the_activation_on_the_same_boot, the_wall_clock_cannot_move_the_guard_while_the_boot_is_the_same, after_a_reboot_the_guard_falls_back_on_the_wall_clock_and_doubt_means_prolong}`.

## Cas limites
- Sans la garde de 30 minutes, le nombre d'essais dépendrait de la fréquence à laquelle l'administrateur bascule l'interrupteur : au plus 16 essais par heure avec elle (deux activations distinctes).
- Une session seule n'a PAS droit à l'essai (BR-TRUST-013).

## Règles liées
- BR-TRUST-011, 013, 014, 015, 016, 020, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
