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
`ServiceStart` `0x0C`, `ServiceStop` `0x0D`, `ServiceRestart` `0x0E` sont dans `ActKind::ALL`, `audit_code` et `covered_by_elevation` (vrai : en ALERTE, l'élévation les couvre), et leurs trois routes sont dans `ROUTES` : la cible signée est la clé du service (décodée du chemin comme l'agent le fait), sans paramètre. L'agent exige donc le droit, puis la preuve de clé du poste (à chaque acte, dans tous les états) et le mot de passe selon l'état de confiance (grille HRT-194) : NORMAL, aucun mot de passe ; ALERTE, mot de passe une fois puis élévation de cinq minutes (réglage « à chaque action » : à chaque acte) ; MODE ATTAQUE (actif ou suspendu), mot de passe à chaque acte. Un acte de pilotage confirmé en NORMAL n'ouvre aucune élévation. Implémenté par HRT-194 (`hearth_proto::admin_act::{is_piloting, password_need}`, pas encore dans le code) ; les pointeurs ci-dessous sont inchangés. Le test de garde `every_modifying_route_is_an_admin_act_or_a_named_exception` reste vert sans exception de plus. La liaison cliente reconstruit l'acte depuis la requête (`hearth-link domain/act.rs`).

## Application (code)
- `crates/hearth-proto/src/admin_act.rs::{ROUTES, ActKind::DECLARED_WITHOUT_ROUTE, covered_by_elevation}`
- `crates/hearth-link/src/domain/act.rs::build`
- `crates/hearth-agent/src/entrypoint/http/reauth.rs::Parsed`
- `deploy/e2e/admin_act.py::confirm_service`
- `crates/hearth-link/src/manager/service_lifecycle.rs::LinkManager::service_gesture` et `apps/desktop/src-tauri/src/service_ops/lifecycle.rs` (client, HRT-65)

## Vérification
- `crates/hearth-proto/src/admin_act.rs::tests::the_operation_routes_are_acts_and_the_declared_ones_have_no_route_yet`
- `crates/hearth-link/src/domain/act.rs::tests::the_lifecycle_acts_sign_the_key_of_the_path`
- `crates/hearth-agent/src/domain/trust/admin_act.rs (couverture par l'élévation)`
- `deploy/e2e/scenario.sh (étape HRT-64)`

## Cas limites
- Annuler la fenêtre de confirmation ne change rien et n'écrit rien (BR-LIFE-017) : rien n'est envoyé.

## Règles liées
- BR-LIFE-001, BR-TRUST-036, BR-TRUST-043

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
- 2026-10-23 : grille d'état de confiance pour le pilotage (HRT-194, décision Voiky 2026-10-20) : démarrer, arrêter, redémarrer ne demandent plus le mot de passe en NORMAL ; ALERTE : mot de passe puis élévation ; MODE ATTAQUE : à chaque acte. `covered_by_elevation` reste la source unique de la couverture en ALERTE (ADR-0032, amendement 2026-10-20). Supprimer un service garde le mot de passe dans tous les états.
