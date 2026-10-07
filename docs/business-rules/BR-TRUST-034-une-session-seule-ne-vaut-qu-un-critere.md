---
id: BR-TRUST-034
domaine: TRUST
titre: Une session présentée seule (sans adresse retenue ni clé) ne vaut qu'un seul critère, jamais deux
statut: partielle
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-034) ; Q12, Q14 (point 7) ; contexts/hearth/tickets/hrt/HRT-24.md ; ADR-0024
maj: 2026-10-07
---

# BR-TRUST-034 : Une session seule ne vaut qu'un critère

## Règle
Une session valide présentée seule vaut **un** critère (BR-TRUST-001 b), jamais deux : elle ne fait pas retenir l'adresse de celui qui la présente (BR-TRUST-007) et ne lui donne pas une clé. En **NORMAL** et en **ALERTE**, une session présentée seule **fonctionne** (Q13, Q14 point 7) ; seul le mode attaque la refusera (HRT-25).

> **Partielle** : le refus d'une session seule en mode attaque est HRT-25. Ici : jamais deux critères, et elle fonctionne en NORMAL et en ALERTE.

## Application (code)
- `crates/hearth-agent/src/application/sessions.rs::SessionService::authenticate_inner` : l'usage d'une session n'apprend aucune adresse ; seule une preuve de clé valide d'une clé inscrite la fait retenir (`TrustService::on_session_proof`).
- `crates/hearth-agent/src/domain/trust/recognition.rs` : aucun critère « session » à la connexion (une connexion ne présente pas de session).

## Vérification
- `tests/device_proof.rs::a_session_alone_never_enrolls_a_device_nor_retains_an_address`.
- `tests/security_alert.rs::nothing_a_deviceless_appliance_does_ever_refuses_a_poste_with_two_criteria_and_the_right_password` (des preuves de session fausses n'effacent ni ne bloquent rien).

## Règles liées
- BR-TRUST-001, BR-TRUST-007, BR-TRUST-035, ADR-0024.

## Historique
- 2026-10-07 : création (HRT-24, session 2026-10-04-hearth-creation, T33).
