---
id: BR-EDIT-006
domaine: EDIT
titre: Une valeur de variable non touchée est reprise de la révision précédente de la description scellée : elle ne t
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-006), HRT-87
maj: 2026-11-14
---

# BR-EDIT-006 — Une valeur de variable non touchée est reprise de la révision précédente de la description scellée : elle ne t

## Règle
Une valeur de variable non touchée est reprise de la révision précédente de la description scellée : elle ne transite jamais par le client et le moteur n'est pas relu.

## Application (code)
- `crates/hearth-agent/src/domain/services/update.rs::apply`

## Vérification
- `crates/hearth-agent/src/domain/services/update.rs::tests::a_kept_value_comes_from_the_description_never_from_the_client`
- `crates/hearth-agent/tests/service_update_https.rs::a_running_service_is_recreated_with_its_kept_values_and_the_old_one_goes_last`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
