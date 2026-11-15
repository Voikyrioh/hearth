---
id: BR-EDIT-008
domaine: EDIT
titre: Un port que le service tient déjà n'est pas un conflit avec lui-même
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-008), HRT-87
maj: 2026-11-14
---

# BR-EDIT-008 — Un port que le service tient déjà n'est pas un conflit avec lui-même

## Règle
Un port que le service tient déjà n'est pas un conflit avec lui-même ; un port nouveau est jugé comme au déploiement.

## Application (code)
- `crates/hearth-agent/src/application/service_update.rs::ServiceUpdates::update`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::the_closed_plan_and_the_validator_refuse_what_a_deployment_refuses`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
