---
id: BR-TRUST-017
domaine: TRUST
titre: Un poste non reconnu reçoit le refus générique d'un mot de passe faux et ne distingue rien (mode actif ou non, essai accordé ou refusé, identifiant existant ou non)
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-017) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-017 : Un poste non reconnu reçoit le refus générique d'un mot de passe faux et ne distingue rien (mode actif ou non, essai accordé ou refusé, identifiant existant ou non)

## Règle
Tous les refus d'un poste non reconnu sortent par le chemin existant du mot de passe faux : la règle fixe deux booléens (`escapes_slowdown`, `password_counts`), le cas d'usage calcule `verified' = verified && password_counts` avant la décision existante. Même code, même corps, mêmes en-têtes, même attente annoncée, mêmes calculs (Argon2 toujours, vérification de la signature sous la clé fournie), mêmes compteurs, mêmes écritures qu'un mot de passe faux ; le journal garde la raison « identifiants incorrects » (jamais « poste non reconnu » pour une connexion). `/hello` et le défi ne changent pas.

Une session refusée reçoit la même réponse qu'une session expirée (BR-TRUST-013).

Reste observable, dit : par qui possède le bon mot de passe (un refus malgré lui lui apprend qu'un blocage le tient dehors), par qui possède un jeton (BR-TRUST-013), par tout le monde (une attente annoncée dit qu'un identifiant est visé), et la ligne d'essai écrite pour un critère présenté (invisible hors de la base).

## Application (code)
- `crates/hearth-agent/src/application/sessions.rs::SessionService::verify`.
- `crates/hearth-agent/src/domain/login_policy.rs::conclude`.
- `crates/hearth-agent/src/entrypoint/http/error.rs`.

## Vérification
- `attack_mode.rs` : `an_unrecognised_device_cannot_tell_the_mode_the_existence_of_the_identifier_or_the_password` (NORMAL, mode attaque, suspendu x identifiant existant ou non x mot de passe faux ou juste : réponse, compteurs, calculs, journal), `a_trial_a_used_trial_and_no_criterion_give_the_same_answer_on_the_wire`.
- `attack_mode_http.rs` : `a_session_alone_is_refused_on_every_route_like_an_unknown_token_and_works_again_at_the_end`.

## Cas limites
- Le chemin n'est pas à durée strictement constante (une vérification Ed25519 quand une preuve est fournie, une ligne d'essai) : de l'ordre de la dizaine de microsecondes sous un Argon2 de plusieurs dizaines de millisecondes. Les tests comptent les appels, ils ne chronomètrent pas.

## Règles liées
- BR-CONN-013, BR-TRUST-001, 013, 016, ADR-0024, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
