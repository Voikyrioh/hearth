---
id: BR-EDIT-033
domaine: EDIT
titre: Chaque modification est journalisée : compte, service, catégories changées, nombres de variables, ports du ser
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-033), HRT-87
maj: 2026-11-14
---

# BR-EDIT-033 — Chaque modification est journalisée : compte, service, catégories changées, nombres de variables, ports du ser

## Règle
Chaque modification est journalisée : compte, service, catégories changées, nombres de variables, ports du serveur, issue et cause typée ; jamais un nom, une valeur, la commande ni un chemin.

## Application (code)
- `crates/hearth-agent/src/domain/audit/event.rs::UpdateNote`

## Vérification
- `crates/hearth-agent/src/domain/audit/event.rs::update_tests::the_entry_says_categories_and_numbers_never_a_name_or_a_value`
- `crates/hearth-agent/tests/service_update_https.rs::a_running_service_is_recreated_with_its_kept_values_and_the_old_one_goes_last`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
