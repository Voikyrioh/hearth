---
id: BR-TRUST-016
domaine: TRUST
titre: En mode attaque, un poste sans aucun critère (ni adresse retenue, ni clé) est bloqué sans aucun essai
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-016) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-016 : En mode attaque, un poste sans aucun critère (ni adresse retenue, ni clé) est bloqué sans aucun essai

## Règle
Aucun critère avant le mot de passe : `password_counts = false`, `trial = None`. Le mot de passe est vérifié quand même (Argon2 toujours, contre le haché factice si l'identifiant n'existe pas) puis traité comme faux : mêmes compteurs, même réponse que pour un mot de passe faux (BR-TRUST-017). Aucune ligne d'essai n'est écrite, pour un identifiant existant ou non.

## Application (code)
- `crates/hearth-agent/src/domain/trust/recognition.rs::judge_login`.
- `crates/hearth-agent/src/application/sessions.rs::SessionService::verify`.

## Vérification
- `attack_mode.rs` : `a_post_without_any_criterion_is_blocked_without_a_trial_even_with_the_right_password`, `without_spoofing_an_attacker_obtains_zero_trial`.

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux des règles liées.

## Règles liées
- BR-TRUST-011, 012, 017, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
