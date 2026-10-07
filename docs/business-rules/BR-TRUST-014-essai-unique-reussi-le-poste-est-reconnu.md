---
id: BR-TRUST-014
domaine: TRUST
titre: Si l'essai unique est réussi du premier coup, le poste atteint deux critères : il est reconnu et connecté
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-014) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-014 : Si l'essai unique est réussi du premier coup, le poste atteint deux critères : il est reconnu et connecté

## Règle
Le mot de passe d'un poste qui n'a qu'un critère et son essai libre est juste : la connexion est accordée (session ouverte), l'**adresse est apprise ou rafraîchie** (BR-TRUST-007), l'essai est consigné `succeeded` et l'entrée `attack_mode.trial` « réussi » est écrite (BR-TRUST-032), dans la transaction de la connexion.

Un essai réussi ne consomme rien : le titulaire qui se reconnecte pendant la même activation (session perdue, déconnexion) entre encore ; seul un mot de passe faux ensuite depuis ce critère le fait passer en essai raté (BR-TRUST-015). Un attaquant n'y gagne rien : sans le mot de passe il ne réussit pas d'essai, avec il est déjà entré.

L'inscription d'un nouveau poste reste **gelée** pendant tout le mode attaque (BR-TRUST-004) : une clé neuve présentée avec un critère admis par l'essai reçoit `device: "deferred"`, la connexion réussit quand même ; le poste est inscrit à la première connexion par mot de passe après la fin du mode. Un poste admis sur la seule adresse (usurpable) n'obtient jamais de clé durable pendant l'attaque.

## Application (code)
- `crates/hearth-agent/src/application/sessions.rs::SessionService::{verify, open_session}` (essai écrit dans la transaction de la tentative, journal publié après validation).
- `crates/hearth-agent/src/application/trust.rs::TrustService::on_login` (inscription gelée, `enrollment_frozen`).

## Vérification
- `attack_mode.rs` : `a_key_alone_after_an_address_change_has_exactly_one_trial`, `an_address_alone_without_key_has_exactly_one_trial` (la reconnexion entre encore), `the_enrolment_is_frozen_in_attack_mode_but_the_login_that_the_trial_admits_still_works`.
- `tests/device_proof.rs::the_enrolment_is_frozen_while_the_attack_mode_is_active`.

## Cas limites
- La session ouverte par l'essai fonctionne (session + adresse retenue) ; une reconnexion depuis la même adresse sans clé, avec le bon mot de passe, entre encore.

## Règles liées
- BR-TRUST-004, 007, 012, 032, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
