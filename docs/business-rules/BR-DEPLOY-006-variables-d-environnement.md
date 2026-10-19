---
id: BR-DEPLOY-006
domaine: DEPLOY
titre: Une variable a un nom (lettres, chiffres, soulignés, sans commencer par un chiffre, unique) et une valeur d'au plus 8 192 caractères sur une seule ligne ; nom et valeur sont des secrets pour l'agent
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-006), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-006 — Une variable a un nom (lettres, chiffres, soulignés, sans commencer par un chiffre, unique) et une valeur d'au plus 8 192 caractères sur une seule ligne ; nom et valeur sont des secrets pour l'agent

## Règle
Nom et valeur sont des `Secret` : jamais affichés (`Debug` masqué), jamais dans une trace, une erreur, le journal d'activité, l'opération ni la signature de l'acte (qui ne porte que leur nombre). Ils ne sont écrits que scellés dans la description durable (BR-EDIT-034) puis chez le moteur.

## Application (code)
- `crates/hearth-agent/src/domain/services/deploy.rs::validate`
- `crates/hearth-agent/src/domain/services/plan.rs::ContainerPlan (Debug)`
- `crates/hearth-proto/src/api/deploy.rs::DeployEnv (Debug)`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::no_variable_value_name_or_command_leaves_the_engine_after_a_success_or_a_failure`
- `crates/hearth-agent/src/domain/services/deploy.rs::tests::the_variables_are_bounded_unique_and_on_one_line`
- `crates/hearth-agent/src/domain/services/deploy.rs::tests::a_refusal_never_carries_the_value_it_refuses`

## Cas limites
- Le NOM des variables d'un service créé par Hearth est lisible de tous les comptes (liste des services) ; la valeur ne l'est que par la révélation (BR-SVC-009).

## Règles liées
BR-EDIT-034, BR-DEPLOY-025, BR-DEPLOY-037

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
