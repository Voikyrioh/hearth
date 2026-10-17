---
id: BR-DEPLOY-032
domaine: DEPLOY
titre: Les vérifications du formulaire (nom, ports, dossiers) sont indicatives ; le serveur les refait toutes au lancement avec la même sévérité
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-032), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-032 — Les vérifications du formulaire (nom, ports, dossiers) sont indicatives ; le serveur les refait toutes au lancement avec la même sévérité

## Règle
`POST /deployments/check` et `POST /deployments` appellent les mêmes fonctions de domaine (`validate`, `verdict`, `name_status`). Un point qui change entre la saisie et le lancement est refusé avec son code, sans rien créer.

## Application (code)
- `crates/hearth-agent/src/application/deployments.rs::Deployments::{check, deploy}`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::the_check_reports_each_field_writes_nothing_and_is_not_journaled`
- `crates/hearth-agent/tests/service_deploy_https.rs::a_port_held_by_a_container_a_program_or_the_agent_is_refused_with_a_suggestion`

## Cas limites
- Aucun cas particulier connu.

## Règles liées
BR-DEPLOY-004, BR-DEPLOY-008

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
