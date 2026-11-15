---
id: BR-EDIT-014
domaine: EDIT
titre: Nom, identité, dossiers non retirés et leur contenu, limites, règle de redémarrage, délai d'arrêt, réglages et
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-014), HRT-87
maj: 2026-11-14
---

# BR-EDIT-014 — Nom, identité, dossiers non retirés et leur contenu, limites, règle de redémarrage, délai d'arrêt, réglages et

## Règle
Nom, identité, dossiers non retirés et leur contenu, limites, règle de redémarrage, délai d'arrêt, réglages et état voulu sont conservés ; le conteneur est recréé.

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
