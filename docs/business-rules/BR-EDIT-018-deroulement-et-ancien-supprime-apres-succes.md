---
id: BR-EDIT-018
domaine: EDIT
titre: Vérification, récupération, arrêt, mise de côté de l'ancien, création du nouveau, démarrage, surveillance
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-018), HRT-87
maj: 2026-11-14
---

# BR-EDIT-018 — Vérification, récupération, arrêt, mise de côté de l'ancien, création du nouveau, démarrage, surveillance

## Règle
Vérification, récupération, arrêt, mise de côté de l'ancien, création du nouveau, démarrage, surveillance ; l'ancien conteneur n'est supprimé qu'après le succès.

## Application (code)
- `crates/hearth-agent/src/domain/ops/update.rs::op_plan`
- `crates/hearth-agent/src/application/service_update.rs::ServiceUpdates::run`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::a_running_service_is_recreated_with_its_kept_values_and_the_old_one_goes_last`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
