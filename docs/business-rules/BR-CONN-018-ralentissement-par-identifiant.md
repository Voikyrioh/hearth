---
id: BR-CONN-018
domaine: CONN
titre: Un identifiant attaqué depuis de nombreuses adresses est ralenti (délai croissant, plafond 2 minutes), jamais bloqué
statut: active
invariant: true
source: contexts/hearth/tickets/hrt/HRT-20.md (critères 1 et 2), ADR-0022, suivi de la review de HRT-04
maj: 2026-10-06
---

# BR-CONN-018 — Ralentissement par identifiant

## Règle
> **Règle nouvelle**, absente de la spécification fonctionnelle : créée par le ticket HRT-20 et l'ADR-0022 (source ci-dessus). **Provisoire pour ce qui touche aux adresses connues** : sera remplacé par l'identité d'appareil par clé (conception à venir).

Un compteur par **identifiant saisi** (empreinte SHA-256 de l'identifiant normalisé, jamais l'identifiant en clair ; qu'il existe ou non), tous clients confondus, compte les échecs de connexion venus d'adresses inconnues de **tous** les comptes. Changer d'adresse ne redonne aucun essai sur l'identifiant visé.

- **10 échecs gratuits**, puis attente avant la tentative suivante : **2 s, 4 s, 8 s, 16 s, 32 s, 64 s, puis 120 s : plafond explicite**. Jamais un blocage : même sous attaque continue, une tentative reste possible à chaque fin d'attente.
- Remise à zéro après **30 minutes sans échec**. Un succès ne la remet pas à zéro.
- Pendant l'attente : `429 TOO_MANY_ATTEMPTS`, `details.retry_after_s`, **la même réponse** que pour toute autre attente. Le refus se fait **après la vérification** du mot de passe (haché factice si l'identifiant n'existe pas), pour que le chemin et la durée soient identiques pour un identifiant existant ou non (BR-CONN-013). Un mot de passe faux compte dans le couple (BR-CONN-006) pour tout le monde ; un mot de passe juste depuis une adresse inconnue du compte est refusé sans rien compter.
- **Exception** : le poste que la règle « 2 critères sur 3 » reconnaît (BR-TRUST-001, BR-CONN-019), avec le bon mot de passe, passe malgré l'attente (HRT-24, ADR-0024). L'alerte (BR-TRUST-008) se déduit de ce compteur.
- Horloge : une attente à plus de 120 s devant `maintenant` (horloge reculée) est ramenée à `maintenant + 120 s` et la correction est écrite ; une horloge avancée met fin à l'attente.
- Table bornée à 10 000 identifiants, ordre d'éviction unique avec `login_attempts` (`domain::eviction::rank` : sans attente en cours d'abord, puis moins d'échecs, puis les plus anciens).

## Application (code)
- `crates/hearth-agent/src/domain/identifier_slowdown.rs::{record_failure, observe, remaining, excess}` ; `domain/login_policy.rs::conclude` ; `domain/lockout.rs::AttemptKey::identifier` ; `domain/eviction.rs::rank`.
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login_in_turn` — lit, décide, écrit dans la même transaction que l'issue.
- `crates/hearth-agent/src/infrastructure/sqlite/login_attempt_repo.rs` — table `identifier_slowdowns` (migration `0004`).

## Vérification
- Domaine : `domain::identifier_slowdown::tests` (gratuits, courbe, plafond, remise à zéro, horloge reculée et avancée, borne), `domain::login_policy::tests`, `domain::eviction::tests`.
- Intégration : `tests/login_lockout.rs::{an_identifier_attacked_from_many_addresses_is_slowed_with_a_growing_delay_capped_at_two_minutes, the_wait_is_never_a_block_a_try_is_always_possible_when_it_ends, thirty_minutes_without_a_failure_give_the_counter_back, the_slowdown_applies_to_an_identifier_that_does_not_exist_too, the_slowdown_is_per_identifier_another_account_is_not_slowed, the_slowdown_and_the_known_addresses_survive_a_restart_of_the_agent, the_identifier_table_is_bounded_and_keeps_the_identifier_really_attacked}` ; `tests/login_review.rs` (horloge, journal, oracle).

## Essais offerts à un attaquant (calcul, ADR-0022)
45 la première heure (10 gratuits, 6 pendant la montée de 126 s, 29 au plafond), puis 30 par heure (31 à 32 en alternant pauses de 30 minutes et salves). Avec usurpation des 8 adresses connues du compte (pire cas) : 133 la première heure puis 62 par heure.

## Limites connues
- Un poste **jamais vu** du compte, pendant une attaque continue contre son identifiant, est ralenti, voire privé d'essais tant que l'attaque dure (il peut perdre la course à chaque fin d'attente). Voies de secours : un poste déjà connu du compte, ou la commande `hearth-agent account` sur le serveur.
- Les limites du mécanisme « adresse connue » (provisoire) : BR-CONN-019.
- Une tentative ralentie coûte un calcul Argon2 (le refus suit la vérification) : borné par le plafond des connexions en cours (BR-CONN-020).
- Le 11e échec écrit « Blocage temporaire » au journal ; les tentatives refusées à cause du ralentissement y sont comptées (changement à valider de BR-AUDIT-007, voir son complément).

## Règles liées
- BR-CONN-006, BR-CONN-007, BR-CONN-013, BR-CONN-019, BR-CONN-020, BR-AUDIT-007.

## Historique
- 2026-10-06 — création (HRT-20, ADR-0022). Corrigée après la review de la PR 23 : refus après vérification, échecs comptés pour les adresses inconnues de tous les comptes, plafond d'horloge, essais par heure recalculés.
