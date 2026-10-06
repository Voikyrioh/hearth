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
> **Règle nouvelle**, absente de la spécification fonctionnelle : créée par le ticket HRT-20 et l'ADR-0022 (source ci-dessus).

Un compteur par **identifiant saisi** (empreinte SHA-256 de l'identifiant normalisé, jamais l'identifiant en clair ; qu'il existe ou non) compte les échecs de connexion venus d'adresses **non connues du compte visé** (BR-CONN-019). Il ferme le contournement par changement d'adresse : changer d'adresse ne redonne aucun essai sur l'identifiant visé.

- **10 échecs gratuits** (le compteur du couple, BR-CONN-006, verrouille déjà au 5e depuis une même adresse : une personne qui se trompe ne le voit jamais).
- Au 11e échec et aux suivants : attente avant la tentative suivante, **2 s, 4 s, 8 s, 16 s, 32 s, 64 s, puis 120 s : plafond explicite** (`MAX_DELAY`). Le délai n'est **jamais** supérieur à 2 minutes et n'est **jamais un blocage** : même sous attaque continue, une tentative reste possible à chaque fin d'attente.
- Le compteur repart à zéro après **30 minutes sans échec**. Un succès ne le remet pas à zéro (comme le compteur par origine, BR-CONN-007).
- Pendant l'attente : `429 TOO_MANY_ATTEMPTS`, `details.retry_after_s` (arrondi au-dessus), **sans vérifier le mot de passe** (donc sans calcul Argon2). C'est **la même réponse** que pour les autres attentes : rien n'indique quel compteur a joué. L'échec qui déclenche l'attente reçoit déjà cette réponse. Une tentative refusée pendant l'attente ne change rien.
- **Jamais appliqué à une adresse connue du compte** (BR-CONN-019) : ni son admission, ni ses échecs ne passent par ce compteur.
- Un identifiant qui n'existe pas est ralenti **exactement comme** un identifiant qui existe (BR-CONN-013) : l'attente ne révèle pas l'existence du compte.
- Horloge : l'attente est une date persistée ; l'attente restante est toujours ramenée au plafond (une horloge reculée ne fabrique pas un blocage long) ; un dernier échec très éloigné dans le futur remet le compteur à zéro.
- Table bornée à 10 000 identifiants : au-delà, on oublie d'abord les moins attaqués (moins d'échecs), les plus anciens. Un flot d'identifiants inventés évince ses propres lignes, pas celle de l'identifiant réellement attaqué. Purge des lignes sans échec depuis 30 minutes par la tâche périodique.

## Application (code)
- `crates/hearth-agent/src/domain/identifier_slowdown.rs::{record_failure, remaining, excess}` — la courbe, fonctions pures horloge en paramètre ; `domain/login_policy.rs::{admit, after_failure}` — quand il s'applique.
- `crates/hearth-agent/src/domain/lockout.rs::AttemptKey::identifier` — clé (empreinte seule).
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login_in_turn` — lit, décide, écrit dans la même transaction que l'issue.
- `crates/hearth-agent/src/infrastructure/sqlite/login_attempt_repo.rs` — table `identifier_slowdowns` (migration `0004`), borne et éviction.

## Vérification
- Domaine : `domain::identifier_slowdown::tests` (gratuits, courbe, plafond, remise à zéro, horloge reculée et avancée, borne) ; `domain::login_policy::tests` (attaque depuis N adresses, adresse connue jamais ralentie).
- Intégration : `tests/audit_use_cases.rs::refused_logins_from_many_addresses_are_all_counted_in_bounded_entries` ; `tests/login_lockout.rs::{an_identifier_attacked_from_many_addresses_is_slowed_with_a_growing_delay_capped_at_two_minutes, the_wait_is_never_a_block_a_try_is_always_possible_when_it_ends, thirty_minutes_without_a_failure_give_the_counter_back, the_slowdown_applies_to_an_identifier_that_does_not_exist_too, the_slowdown_is_per_identifier_another_account_is_not_slowed, the_slowdown_and_the_known_addresses_survive_a_restart_of_the_agent, the_identifier_table_is_bounded_and_keeps_the_identifier_really_attacked}`.

## Cas limites et limites connues
- Un utilisateur légitime sur une adresse **nouvelle**, pendant une attaque soutenue sur son identifiant, peut perdre la course contre l'attaquant au moment où l'attente expire : il n'est pas bloqué définitivement, il peut attendre longtemps. Parade : se connecter d'abord depuis un poste connu, ou la ligne de commande sur le serveur (`hearth-agent account`).
- Débit maximal contre un identifiant (attaque depuis N adresses) : environ 60 essais par heure (10 gratuits, puis un toutes les 2 minutes ; 10 de plus à chaque fenêtre de 30 minutes sans échec).
- Le déclenchement d'une attente écrit l'entrée « Blocage temporaire » du journal, et chaque tentative refusée à cause du ralentissement est comptée (une entrée puis une synthèse au compte exact), sans champ libre (BR-AUDIT-007, complément HRT-20).

## Règles liées
- BR-CONN-006, BR-CONN-007, BR-CONN-013, BR-CONN-019, BR-AUDIT-007.

## Historique
- 2026-10-06 — création (HRT-20, ADR-0022).
