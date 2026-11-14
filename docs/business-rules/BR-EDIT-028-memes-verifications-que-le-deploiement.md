---
id: BR-EDIT-028
domaine: EDIT
titre: Le serveur refait toutes les vérifications avec la sévérité du déploiement
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-028), HRT-87
maj: 2026-11-14
---

# BR-EDIT-028 — Le serveur refait toutes les vérifications avec la sévérité du déploiement

## Règle
Le serveur refait toutes les vérifications avec la sévérité du déploiement ; un membre hors du plan fermé est refusé `PLAN_SETTING_REFUSED`.

## Application (code)
- `crates/hearth-agent/src/domain/services/update.rs::apply`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::the_closed_plan_and_the_validator_refuse_what_a_deployment_refuses`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
