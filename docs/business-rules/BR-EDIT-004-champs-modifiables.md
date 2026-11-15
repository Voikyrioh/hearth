---
id: BR-EDIT-004
domaine: EDIT
titre: Variables, ports, dossiers, commande, version de l'image et adresse des joueurs se modifient
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-004), HRT-87
maj: 2026-11-14
---

# BR-EDIT-004 — Variables, ports, dossiers, commande, version de l'image et adresse des joueurs se modifient

## Règle
Variables, ports, dossiers, commande, version de l'image et adresse des joueurs se modifient ; le nom, le dépôt de l'image, la règle de redémarrage, le délai d'arrêt et les limites sont conservés.

## Application (code)
- `crates/hearth-agent/src/domain/services/update.rs::apply`

## Vérification
- `crates/hearth-agent/src/domain/services/update.rs::tests::what_the_modification_keeps_is_kept`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
