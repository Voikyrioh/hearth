---
id: BR-TRUST-006
domaine: TRUST
titre: En état d'alerte, un poste reconnu n'est pas ralenti par les échecs des autres appareils ; un poste non reconnu l'est, sans être bloqué
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-006) ; Q13 ; contexts/hearth/tickets/hrt/HRT-24.md ; ADR-0024
maj: 2026-10-07
---

# BR-TRUST-006 : En alerte, un poste reconnu n'est pas ralenti

## Règle
En **ALERTE** (l'identifiant est visé : il est ralenti, BR-CONN-018 ; mode attaque non activé), un poste reconnu (BR-TRUST-001) n'est pas ralenti par les échecs que d'autres appareils accumulent contre son identifiant : avec le bon mot de passe il passe sans attendre. Un poste non reconnu subit le ralentissement. **Personne n'est bloqué** : l'attente est plafonnée à 2 minutes, un mot de passe juste passe entre deux attentes, une session présentée seule fonctionne (Q14, point 7).

## Application (code)
- `crates/hearth-agent/src/domain/trust/recognition.rs::judge_login` (`escapes_slowdown`) ; `domain/login_policy.rs::conclude` (branche « identifiant ralenti »).

## Vérification
- `tests/security_alert.rs::{a_recognised_poste_is_not_slowed_while_other_devices_pile_up_failures, in_alert_every_combination_of_criteria_is_recognised_as_the_table_says_and_nobody_is_locked_out}` (les non reconnus sont ralentis de 1 à 120 s, jamais refusés ensuite).
- `domain::login_policy::tests::the_owner_of_the_password_on_a_known_address_passes_during_the_attack`.

## Règles liées
- BR-TRUST-001, BR-TRUST-035, BR-CONN-018, BR-CONN-019, ADR-0024.

## Historique
- 2026-10-07 : création (HRT-24, session 2026-10-04-hearth-creation, T33).
