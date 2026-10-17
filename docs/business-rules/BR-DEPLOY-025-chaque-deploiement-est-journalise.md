---
id: BR-DEPLOY-025
domaine: DEPLOY
titre: Chaque déploiement terminé, réussi ou non, est inscrit au journal d'activité avec une cause typée, sans variable ni commande
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-025), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-025 — Chaque déploiement terminé, réussi ou non, est inscrit au journal d'activité avec une cause typée, sans variable ni commande

## Règle
Compte, action « Déploiement d'un service », cible (nom, image et version, ports du serveur, NOMBRE de variables, « commande personnalisée »), résultat et cause dans une liste fermée (`Reason`). Jamais un nom ou une valeur de variable, jamais la commande, jamais un chemin. Fermer le formulaire sans lancer n'écrit rien.

## Application (code)
- `crates/hearth-agent/src/domain/audit/event.rs::Target::ServiceDeploy`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::record`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_minimal_deployment_pulls_creates_starts_and_publishes_a_service_created_by_hearth`
- `crates/hearth-agent/tests/service_deploy_https.rs::no_variable_value_name_or_command_leaves_the_engine_after_a_success_or_a_failure`

## Cas limites
- Les refus synchrones (nom pris, port, validation) sont écrits par la couche d'accès sous l'action `service.deploy`.

## Règles liées
BR-DEPLOY-006, BR-DEPLOY-019

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
