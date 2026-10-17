---
id: BR-DEPLOY-030
domaine: DEPLOY
titre: Si le droit de gérer les services est retiré entre l'ouverture du formulaire et le lancement, le serveur refuse la demande suivante ; un déploiement déjà accepté va à son terme
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-030), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-030 — Si le droit de gérer les services est retiré entre l'ouverture du formulaire et le lancement, le serveur refuse la demande suivante ; un déploiement déjà accepté va à son terme

## Règle
Le droit est jugé par la couche d'accès à chaque demande (BR-GRANT-010). L'acte signé ne porte ni valeur, ni nom de variable, ni commande : une signature observée ne permet pas de tester un secret hors ligne (ADR-0031).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/auth.rs::allows`
- `crates/hearth-proto/src/admin_act.rs::AdminAct::service_deploy`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_right_withdrawn_while_the_deployment_runs_does_not_stop_it_but_the_next_request_is_refused`
- `crates/hearth-proto/src/api/service_spec.rs::tests::the_digest_moves_with_every_non_secret_part_and_ignores_every_secret`

## Cas limites
- Aucun cas particulier connu.

## Règles liées
BR-GRANT-010, BR-DEPLOY-001

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
