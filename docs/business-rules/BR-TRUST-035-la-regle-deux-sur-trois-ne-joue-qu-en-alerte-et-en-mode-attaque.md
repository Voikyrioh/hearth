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

> **Partielle** : HRT-22 n'applique la règle dans **aucun** état. La clé est enregistrée, prouvée et listée, et ne change encore aucune décision d'accès : le comportement de connexion est celui de la branche principale, avec ou sans clé. La règle en alerte (HRT-24) et en mode attaque (HRT-25) viendront avec leurs tickets ; l'état normal, lui, ne sera jamais touché.

## Application (code)
- HRT-22 : aucune décision ne lit la clé. `crates/hearth-agent/src/application/sessions.rs::SessionService::login_in_turn` appelle `login_policy::conclude` avec les mêmes entrées qu'avant ; la clé n'intervient qu'**après** un succès, pour inscrire ou dater le poste (`application/trust.rs::TrustService::on_login`).
- La fonction pure de la règle (`domain/trust/recognition.rs`) est HRT-24.

## Vérification
- Tous les tests de connexion existants (`tests/login_lockout.rs`, `tests/login_review.rs`, `tests/sessions_use_cases.rs`, `tests/sessions_https.rs`, `tests/http_api.rs`) tournent, **sans être modifiés**, avec l'identité d'appareil branchée (`tests/support/mod.rs::env`).
- `tests/device_proof.rs::a_right_password_with_a_false_or_missing_proof_connects_as_if_there_were_no_key` ; `tests/device_http.rs::a_wrong_password_answers_the_same_with_a_valid_proof_a_false_one_or_none`.

## Cas limites
- Un client qui n'envoie pas de clé (le client actuel) se connecte comme avant, dans tous les états.

## Règles liées
- BR-TRUST-004, BR-TRUST-005, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
