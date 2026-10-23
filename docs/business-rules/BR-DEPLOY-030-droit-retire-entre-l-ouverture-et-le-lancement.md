---
id: BR-DEPLOY-030
domaine: DEPLOY
titre: Si le droit de gérer les services est retiré entre l'ouverture du formulaire et le lancement, le serveur refuse la demande suivante ; un déploiement déjà accepté va à son terme
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-030), HRT-68
maj: 2026-10-24
---

# BR-DEPLOY-030 — Si le droit de gérer les services est retiré entre l'ouverture du formulaire et le lancement, le serveur refuse la demande suivante ; un déploiement déjà accepté va à son terme

## Règle
Le droit est jugé par la couche d'accès à chaque demande (BR-GRANT-010). L'acte signé ne porte ni valeur, ni nom de variable, ni commande : une signature observée ne permet pas de tester un secret hors ligne (ADR-0031).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/auth.rs::allows`
- `crates/hearth-proto/src/admin_act.rs::AdminAct::service_deploy`
- `apps/desktop/src/composables/useDeployFlow.ts::perform` et `composables/useServerAction.ts` (client : « Vous n'avez plus le droit… », la saisie reste)
- `crates/hearth-link/src/manager/service_deploy.rs::deploy_service` (acte `ServiceDeploy`, jamais couvert par l'élévation)
- `apps/desktop/src-tauri/src/service_ops/deploy.rs::interpret_deploy` (droit retiré : `MissingServicesRight`) et `reauth/service.rs::act_of` (`service_deploy` : mot de passe à chaque fois)

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_right_withdrawn_while_the_deployment_runs_does_not_stop_it_but_the_next_request_is_refused`
- `crates/hearth-proto/src/api/service_spec.rs::tests::the_digest_moves_with_every_non_secret_part_and_ignores_every_secret`
- `apps/desktop/src/pages/DeployFlow.test.ts (le droit retiré entre l'ouverture et le lancement)`
- `crates/hearth-link/tests/service_deploy.rs::a_deployment_sent_by_the_link_is_signed_like_the_agent_reads_it_and_never_without_the_password`
- `apps/desktop/src-tauri/src/reauth/service.rs::tests::the_password_need_is_the_grid_of_the_agent_for_piloting_and_the_elevation_table_for_the_rest`

## Cas limites
- Aucun cas particulier connu.

## Règles liées
BR-GRANT-010, BR-DEPLOY-001

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
- 2026-10-24 — côté client (HRT-69, déployer une image).
- 2026-10-24 — coquille Rust et liaison (HRT-69, déployer une image).
