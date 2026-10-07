---
id: BR-TRUST-035
domaine: TRUST
titre: La règle « 2 sur 3 » ne s'applique qu'en alerte et en mode attaque ; en état normal aucun poste n'est ralenti ni bloqué au nom de cette règle
statut: partielle
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-035, Q13) ; contexts/hearth/tickets/hrt/HRT-22.md ; ADR-0023
maj: 2026-10-07
---

# BR-TRUST-035 : La règle « 2 sur 3 » ne joue qu'en alerte et en mode attaque

## Règle
La règle « 2 critères sur 3 » (adresse retenue, session ou mot de passe du premier coup, clé de l'appareil) ne s'applique que dans deux états : l'**alerte** (attaque probable signalée sur l'identifiant) et le **mode attaque**. En état **normal** elle ne joue pas : une session valide fonctionne, une connexion par mot de passe se fait comme d'habitude, et aucun poste n'est ralenti ni bloqué au nom de cette règle (Voiky, Q13).

> **Partielle** : HRT-24 applique la règle en **ALERTE** (BR-TRUST-001, 006) et la laisse sans effet en **NORMAL** (`judge_login(Mode::Normal, _)` rend toujours `escapes_slowdown = false, password_counts = true`, et l'identifiant n'est pas ralenti : aucun comportement ne change). Le mode attaque est HRT-25 ; l'état normal, lui, ne sera jamais touché.

## Application (code)
- `crates/hearth-agent/src/domain/trust/recognition.rs::{judge_login, mode_of}` (HRT-24) ; `application/sessions.rs::SessionService::verify` appelle `judge_login` puis `login_policy::conclude`. La clé enregistre ou date le poste **après** un succès (`application/trust.rs::TrustService::on_login`, HRT-22).

## Vérification
- Tous les tests de connexion existants (`tests/login_lockout.rs`, `tests/sessions_use_cases.rs`, `tests/sessions_https.rs`) tournent sans être modifiés avec l'identité d'appareil et l'alerte branchées (`tests/support/mod.rs::env`) ; les adaptations de HRT-24 (BR-AUDIT-007, BR-CONN-019, BR-TRUST-022) sont listées dans l'ADR-0024.
- `tests/security_alert.rs::in_the_normal_state_the_rule_changes_nothing_a_stranger_connects_like_a_known_poste` ; `domain::trust::recognition::tests::in_the_normal_state_the_rule_changes_nothing_whatever_the_criteria`.
- `tests/device_proof.rs::a_right_password_with_a_false_or_missing_proof_connects_as_if_there_were_no_key` ; `tests/device_http.rs::a_wrong_password_answers_the_same_with_a_valid_proof_a_false_one_or_none`.

## Cas limites
- Un client qui n'envoie pas de clé (le client actuel) se connecte comme avant, dans tous les états.

## Règles liées
- BR-TRUST-004, BR-TRUST-005, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
- 2026-10-07 : HRT-24, la règle joue en ALERTE, jamais en NORMAL (T33).
