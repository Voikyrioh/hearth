---
id: BR-EDIT-010
domaine: EDIT
titre: La commande est gardée, posée (découpée, jamais passée à un interpréteur) ou vidée (celle de l'image)
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-010), HRT-87
maj: 2026-11-14
---

# BR-EDIT-010 — La commande est gardée, posée (découpée, jamais passée à un interpréteur) ou vidée (celle de l'image)

## Règle
La commande est gardée, posée (découpée, jamais passée à un interpréteur) ou vidée (celle de l'image).

## Application (code)
- `crates/hearth-agent/src/domain/services/update.rs::apply`

## Vérification
- `crates/hearth-agent/src/domain/services/update.rs::tests::the_command_is_set_cleared_or_kept`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
