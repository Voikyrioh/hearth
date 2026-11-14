---
id: BR-EDIT-021
domaine: EDIT
titre: Un échec avant l'arrêt (validation, récupération, dossier, port, annulation) ne touche à rien
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-021), HRT-87
maj: 2026-11-14
---

# BR-EDIT-021 — Un échec avant l'arrêt (validation, récupération, dossier, port, annulation) ne touche à rien

## Règle
Un échec avant l'arrêt (validation, récupération, dossier, port, annulation) ne touche à rien.

## Application (code)
- `crates/hearth-agent/src/application/service_update.rs::ServiceUpdates::update`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::a_failed_pull_touches_nothing`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
