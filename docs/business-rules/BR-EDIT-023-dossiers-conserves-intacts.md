---
id: BR-EDIT-023
domaine: EDIT
titre: Une modification ne crée, ne déplace, n'écrase ni n'efface aucun fichier des dossiers conservés
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-023), HRT-87
maj: 2026-11-14
---

# BR-EDIT-023 — Une modification ne crée, ne déplace, n'écrase ni n'efface aucun fichier des dossiers conservés

## Règle
Une modification ne crée, ne déplace, n'écrase ni n'efface aucun fichier des dossiers conservés ; elle ne crée que les sous-dossiers gérés ajoutés.

## Application (code)
- `crates/hearth-agent/src/application/service_update.rs::ServiceUpdates::prepare_folders`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::a_running_service_is_recreated_with_its_kept_values_and_the_old_one_goes_last`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
