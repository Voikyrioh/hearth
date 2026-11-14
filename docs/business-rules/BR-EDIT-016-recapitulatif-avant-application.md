---
id: BR-EDIT-016
domaine: EDIT
titre: Le récapitulatif montre les catégories changées (jamais une valeur ni la commande)
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-016), HRT-87
maj: 2026-11-14
---

# BR-EDIT-016 — Le récapitulatif montre les catégories changées (jamais une valeur ni la commande)

## Règle
Le récapitulatif montre les catégories changées (jamais une valeur ni la commande) ; les points d'attention sont les cases `accept`. Côté client (HRT-88).

## Application (code)
- `crates/hearth-agent/src/domain/services/update.rs::Summary`

## Vérification
- `crates/hearth-agent/src/domain/services/update.rs::tests::a_variable_is_added_replaced_or_removed_and_counted_without_a_name`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
