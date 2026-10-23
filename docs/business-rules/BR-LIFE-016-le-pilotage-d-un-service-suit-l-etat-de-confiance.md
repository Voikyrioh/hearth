---
id: BR-LIFE-016
domaine: LIFE
titre: Démarrer, arrêter et redémarrer sont des actes de pilotage : le mot de passe suit l'état de confiance
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-016), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-23
---

# BR-LIFE-016 — Démarrer, arrêter et redémarrer sont des actes de pilotage : le mot de passe suit l'état de confiance

## Règle
`ServiceStart` `0x0C`, `ServiceStop` `0x0D`, `ServiceRestart` `0x0E` (et `OperationDecide` `0x1D`, BR-OPS-003) sont des actes de pilotage (`hearth_proto::admin_act::is_piloting`, correspondance exhaustive sans `_`) ; `OperationCancel` n'en est pas un. Ils sont dans `ActKind::ALL`, `audit_code` et `covered_by_elevation` (vrai pour les trois gestes : c'est la règle d'un agent qui n'annonce pas son état de confiance, BR-COMPAT-001), et leurs trois routes sont dans `ROUTES` : la cible signée est la clé du service (décodée du chemin comme l'agent le fait), sans paramètre. L'agent exige donc le droit, puis la preuve de clé du poste (à chaque acte, dans tous les états) et le mot de passe selon l'état de confiance (grille HRT-194) : NORMAL, aucun mot de passe ; ALERTE, mot de passe une fois puis élévation de cinq minutes (réglage « à chaque action » : à chaque acte) ; MODE ATTAQUE (actif ou suspendu), mot de passe à chaque acte. Un acte de pilotage confirmé en NORMAL n'ouvre aucune élévation, même si un mot de passe superflu est fourni (il est vérifié : faux, refusé ; juste, l'acte passe sans rien ouvrir). L'état de confiance est jugé par l'agent à l'acte, après la preuve de clé (`domain/trust/act_trust.rs::level` : mode attaque actif ou suspendu, sinon alerte de l'IDENTIFIANT du compte, sinon normal) ; le client ne le fournit jamais, il lit `AdminReauthInfo.trust`. Valeur inconnue : mode attaque ; champ absent (agent d'avant) : la règle d'avant. Le niveau jugé est tracé à chaque acte (trace `info` de l'agent : acte, `trust`, `need` ; ni compte ni secret ; le journal d'activité ne change pas). `alert.own` de `GET /security` et `admin_reauth.trust` viennent du même calcul (`application/security.rs::identifier_alert`). La décision est `hearth_proto::admin_act::password_need` (source unique de l'agent et du client). Le test de garde `every_modifying_route_is_an_admin_act_or_a_named_exception` reste vert sans exception de plus. La liaison cliente reconstruit l'acte depuis la requête (`hearth-link domain/act.rs`).

## Application (code)
- `crates/hearth-proto/src/admin_act.rs::{ROUTES, ActKind::DECLARED_WITHOUT_ROUTE, covered_by_elevation, is_piloting, password_need, TrustLevel, PasswordNeed}`
- `crates/hearth-agent/src/domain/trust/act_trust.rs::level`
- `crates/hearth-agent/src/application/sessions.rs::{reauthenticate, admin_reauth_info}`
- `crates/hearth-link/src/manager/reauth.rs::password_always_needed` (ceinture)
- `apps/desktop/src-tauri/src/reauth/service.rs::password_need_of` et `apps/desktop/src/composables/useReauth.ts`
- `crates/hearth-link/src/domain/act.rs::build`
- `crates/hearth-agent/src/entrypoint/http/reauth.rs::Parsed`
- `deploy/e2e/admin_act.py::confirm_service`
- `crates/hearth-link/src/manager/service_lifecycle.rs::LinkManager::service_gesture` et `apps/desktop/src-tauri/src/service_ops/lifecycle.rs` (client, HRT-65)

## Vérification
- `crates/hearth-proto/src/admin_act.rs::tests::the_operation_routes_are_acts_and_the_declared_ones_have_no_route_yet`
- `crates/hearth-link/src/domain/act.rs::tests::the_lifecycle_acts_sign_the_key_of_the_path`
- `crates/hearth-proto/src/admin_act.rs::tests::{the_password_need_follows_the_trust_grid_for_piloting_and_the_elevation_table_for_the_rest, only_the_four_piloting_acts_are_piloting}`
- `crates/hearth-agent/src/domain/trust/act_trust.rs::tests` et `domain/trust/admin_act.rs (couverture par l'élévation)`
- `crates/hearth-agent/tests/admin_reauth.rs` : `in_the_normal_state_piloting_needs_no_password_opens_no_elevation_and_ignores_the_each_setting`, `in_the_alert_state_the_first_piloting_act_asks_the_password_then_five_minutes_cover_the_next_ones`, `in_the_alert_state_with_the_each_setting_every_piloting_act_asks_the_password`, `in_the_attack_mode_active_or_suspended_every_piloting_act_asks_the_password_and_no_elevation_opens`, `the_proof_of_the_key_is_required_for_a_piloting_act_in_every_state`
- `crates/hearth-link/tests/service_lifecycle.rs::a_gesture_needs_no_password_in_the_normal_state_and_is_not_sent_without_one_in_the_attack_mode`
- Vitest `pages/ServicesLifecycle.test.ts`, `link/simulated-reauth.test.ts` ; Playwright `e2e/hrt65-lifecycle.spec.ts`
- `deploy/e2e/scenario.sh (étape HRT-64)`

## Cas limites
- Annuler la fenêtre de confirmation ne change rien et n'écrit rien (BR-LIFE-017) : rien n'est envoyé.

## Règles liées
- BR-LIFE-001, BR-TRUST-036, BR-TRUST-042, BR-TRUST-043, BR-TRUST-052, BR-OPS-003, BR-GRANT-017

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
- 2026-10-23 : grille d'état de confiance pour le pilotage (HRT-194, décision Voiky 2026-10-20) : démarrer, arrêter, redémarrer ne demandent plus le mot de passe en NORMAL ; ALERTE : mot de passe puis élévation ; MODE ATTAQUE : à chaque acte. `covered_by_elevation` reste la source unique de la couverture en ALERTE (ADR-0032, amendement 2026-10-20). Supprimer un service garde le mot de passe dans tous les états.
- 2026-10-23 : HRT-194 livré (`is_piloting`, `password_need`, `TrustLevel`, `AdminReauthInfo.trust`, `act_trust::level`) ; fichier renommé (ancien nom « couverts par l'élévation »).
