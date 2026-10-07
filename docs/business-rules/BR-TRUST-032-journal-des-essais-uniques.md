---
id: BR-TRUST-032
domaine: TRUST
titre: Le journal d'activité enregistre chaque essai unique accordé en mode attaque et son issue, sans révéler si l'identifiant existe
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-032) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-032 : Le journal d'activité enregistre chaque essai unique accordé en mode attaque et son issue, sans révéler si l'identifiant existe

## Règle
`attack_mode.trial` dans la transaction de la tentative : résultat « réussi », ou « refusé : identifiants incorrects » ; cible « essai sur l'adresse retenue » ou « essai sur la clé du poste » (jamais l'adresse ni la clé). Les essais ne concernent par construction que des comptes existants (un critère présenté implique un compte), et un identifiant inexistant n'écrit aucune entrée d'essai. Borné : 16 essais au plus par compte et par activation. Le journal n'est lisible que des administrateurs (BR-AUDIT-001).

## Application (code)
- `crates/hearth-agent/src/application/sessions.rs::SessionService::verify` (`AuditAction::AttackModeTrial`, `Target::Trial`).

## Vérification
- `attack_mode.rs` : `the_trial_is_journaled_with_its_outcome_and_never_a_secret`, `without_spoofing_an_attacker_obtains_zero_trial`.

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux des règles liées.

## Règles liées
- BR-TRUST-012, 014, 015, 030, BR-AUDIT-005, BR-AUDIT-006, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
