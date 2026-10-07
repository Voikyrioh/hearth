---
id: BR-TRUST-002
domaine: TRUST
titre: Être reconnu ne remplace jamais l'authentification : le mot de passe est toujours demandé et doit être juste
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-002) ; contexts/hearth/tickets/hrt/HRT-24.md ; ADR-0024
maj: 2026-10-07
---

# BR-TRUST-002 : L'authentification reste toujours requise

## Règle
Être reconnu (BR-TRUST-001) ne remplace jamais l'authentification. Le mot de passe est toujours demandé et doit toujours être saisi correctement, quel que soit le poste. La règle « 2 critères sur 3 » **ne rend jamais « accordé »** : elle fixe seulement `escapes_slowdown` et `password_counts`. Seule la décision de connexion (`login_policy::conclude`) accorde, sur un mot de passe **vérifié**. Argon2 est calculé dans tous les cas, y compris pour un poste reconnu, un identifiant inexistant et un poste non reconnu.

## Application (code)
- `crates/hearth-agent/src/domain/trust/recognition.rs::judge_login` (ne rend aucun verdict d'accès) ; `domain/login_policy.rs::conclude` (`verified` exigé).
- `crates/hearth-agent/src/application/sessions.rs::SessionService::verify`.

## Vérification
- `tests/security_alert.rs::in_alert_every_combination_of_criteria_is_recognised_as_the_table_says_and_nobody_is_locked_out` (un mot de passe faux depuis un poste reconnu ne connecte jamais : `a_device_without_account_sees_the_same_answer_whether_the_identifier_exists_or_not_in_every_state`, qui compte un calcul Argon2 par tentative dans tous les états).
- `domain::trust::recognition::tests::the_rule_never_discards_the_password_in_the_states_of_this_ticket`.

## Cas limites
- Un poste reconnu qui se trompe de mot de passe est refusé comme les autres (attente annoncée comprise).

## Règles liées
- BR-TRUST-001, BR-CONN-013, ADR-0024.

## Historique
- 2026-10-07 : création (HRT-24, session 2026-10-04-hearth-creation, T33).
