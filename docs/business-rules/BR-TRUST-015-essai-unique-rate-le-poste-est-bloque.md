---
id: BR-TRUST-015
domaine: TRUST
titre: Si l'essai unique est raté, le poste est bloqué et ne peut plus essayer tant que le mode attaque dure
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-015) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-015 : Si l'essai unique est raté, le poste est bloqué et ne peut plus essayer tant que le mode attaque dure

## Règle
Un mot de passe faux sur l'essai unique (ou, ensuite, depuis un critère dont l'essai avait réussi) : l'essai est consommé (`failed`, la ligne réussie d'avant est mise à jour), les compteurs de connexion avancent comme pour tout mot de passe faux, l'entrée `attack_mode.trial` « refusé, identifiants incorrects » est écrite. Toute tentative suivante depuis ce critère a `password_counts = false` : le mot de passe, **même juste**, est traité comme faux, jusqu'à la fin de l'activation (fin manuelle, automatique, ou commande locale). Le poste n'est pas banni au-delà : à la fin du mode, le chemin ordinaire revient.

Un attaquant peut brûler l'essai d'un poste légitime en usurpant son adresse (au plus un essai par adresse retenue) : c'est le risque assumé de Q11, borné par BR-TRUST-012, et c'est ce que les voies de BR-TRUST-027 couvrent.

## Application (code)
- `crates/hearth-agent/src/domain/trust/recognition.rs::judge_login` (`password_counts: false`).
- `crates/hearth-agent/src/application/sessions.rs::SessionService::verify`.

## Vérification
- `attack_mode.rs` : `a_wrong_password_on_the_single_trial_blocks_the_post_and_the_right_one_no_longer_counts`, `a_key_alone_that_misses_its_trial_is_blocked_until_the_end_of_the_mode_even_with_the_right_password`.

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux des règles liées.

## Règles liées
- BR-TRUST-012, 016, 017, 019, 027, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
