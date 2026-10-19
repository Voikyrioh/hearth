---
id: BR-DEPLOY-019
domaine: DEPLOY
titre: Un échec avant que le programme ait pu se lancer ne laisse aucun service, aucune description, aucun dossier ni conteneur orphelin ; un échec après le lancement conserve le service
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-019), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-019 — Un échec avant que le programme ait pu se lancer ne laisse aucun service, aucune description, aucun dossier ni conteneur orphelin ; un échec après le lancement conserve le service

## Règle
Port pris au dernier moment, refus du moteur, nom pris, image absente, annulation, délai : la description (révision 1) est retirée, le dossier de données du service aussi, un conteneur créé puis refusé au démarrage est supprimé et oublié. Un résultat INCONNU (le moteur ne répond plus après l'ordre) ne retire rien : l'opération est `interrupted` et le compte relit l'état réel. Un agent redémarré avant la création retire la description du déploiement abandonné.

## Application (code)
- `crates/hearth-agent/src/application/deployments.rs::Deployments::settle`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::remove_after_failure`
- `crates/hearth-agent/src/application/service_ops.rs::ServiceOps::recover_at_startup (`abandoned_deploys`)`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_container_refused_at_start_is_removed_and_nothing_stays`
- `crates/hearth-agent/tests/service_deploy_https.rs::an_unreachable_engine_after_the_order_leaves_an_unknown_result_and_removes_nothing`
- `crates/hearth-agent/tests/service_deploy_https.rs::every_pull_failure_is_told_clearly_and_leaves_nothing_behind`

## Cas limites
- Un conteneur dont la suppression échoue après un démarrage refusé reste consigné (listé, supprimable) : rien n'est retiré sous lui.

## Règles liées
BR-DEPLOY-037, BR-DEPLOY-028

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
