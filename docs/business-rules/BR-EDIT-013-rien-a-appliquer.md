---
id: BR-EDIT-013
domaine: EDIT
titre: Une demande qui ne change rien n'applique rien : `200 { applied: false }`, aucune opération, aucune entrée au 
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-013), HRT-87
maj: 2026-11-14
---

# BR-EDIT-013 — Une demande qui ne change rien n'applique rien : `200 { applied: false }`, aucune opération, aucune entrée au 

## Règle
Une demande qui ne change rien n'applique rien : `200 { applied: false }`, aucune opération, aucune entrée au journal.

## Application (code)
- `crates/hearth-agent/src/domain/services/update.rs::apply`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::nothing_to_apply_creates_no_operation_and_no_entry`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
