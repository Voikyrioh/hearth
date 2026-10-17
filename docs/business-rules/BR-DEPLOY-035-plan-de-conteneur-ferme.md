---
id: BR-DEPLOY-035
domaine: DEPLOY
titre: Le serveur refuse tout déploiement dont la demande contient un réglage hors du plan fermé, sans le nommer
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-035), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-035 — Le serveur refuse tout déploiement dont la demande contient un réglage hors du plan fermé, sans le nommer

## Règle
La demande n'a aucun champ pour un mode privilégié, un réseau du serveur, un périphérique ou une option de sécurité ; un membre inconnu du corps est refusé (`422 PLAN_SETTING_REFUSED`, « Ce réglage n'est pas permis pour un service créé par Hearth. ») sans rien créer. Le conteneur est construit par un seul constructeur depuis la description durable (`plan_from_spec`), dont les champs sont la liste entière de ce que l'agent demande au moteur.

## Application (code)
- `crates/hearth-agent/src/domain/services/plan.rs::{ContainerPlan, plan_from_spec}`
- `crates/hearth-agent/src/domain/services/deploy.rs::validate`
- `crates/hearth-proto/src/api/deploy.rs::DeploySpec (`extra`)`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_setting_outside_the_closed_plan_is_refused_without_naming_it`
- `crates/hearth-agent/src/domain/services/plan.rs::tests::the_plan_has_no_field_for_the_options_a_privileged_service_would_need`

## Cas limites
- Un client plus récent qui envoie un champ additif à un agent plus ancien est refusé : c'est voulu pour un plan fermé.

## Règles liées
BR-DEPLOY-012

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
