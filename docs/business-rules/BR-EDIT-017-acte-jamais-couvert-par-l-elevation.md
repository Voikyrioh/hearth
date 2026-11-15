---
id: BR-EDIT-017
domaine: EDIT
titre: `ServiceUpdate` (`0x13`) n'est jamais couvert par l'élévation : le mot de passe à chaque fois
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-017), HRT-87
maj: 2026-11-14
---

# BR-EDIT-017 — `ServiceUpdate` (`0x13`) n'est jamais couvert par l'élévation : le mot de passe à chaque fois

## Règle
`ServiceUpdate` (`0x13`) n'est jamais couvert par l'élévation : le mot de passe à chaque fois.

## Application (code)
- `crates/hearth-proto/src/admin_act.rs::covered_by_elevation`

## Vérification
- `crates/hearth-agent/tests/admin_reauth.rs::every_admin_act_with_a_valid_confirmation_succeeds_and_leaves_one_success_entry`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
