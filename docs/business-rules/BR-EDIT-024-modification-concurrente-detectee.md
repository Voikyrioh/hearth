---
id: BR-EDIT-024
domaine: EDIT
titre: Une modification faite hors de Hearth ou par un autre compte est détectée, jamais écrasée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-024), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-EDIT-024 — Une modification faite hors de Hearth ou par un autre compte est détectée, jamais écrasée

## Règle
Une écriture de la description porte la révision que l'appelant a lue (`expected_rev`) : si la révision courante est autre, l'écriture est refusée (`SpecStoreError::Conflict`, code `SPEC_CONFLICT`, 409) et rien n'est écrit. Un conteneur modifié à la main est affiché « Modifié hors de Hearth » (BR-EDIT-036). Le texte « Ce service a changé depuis que tu as ouvert le formulaire. » et son affichage sont ceux de la modification (HRT-87, HRT-88).

## Application (code)
- `crates/hearth-agent/src/infrastructure/sqlite/specs_repo.rs::SqliteSpecRepo::append_revision`
- `crates/hearth-proto/src/error.rs::ErrorCode::SpecConflict`

## Vérification
- `crates/hearth-agent/tests/service_spec_secrets.rs::a_concurrent_modification_and_a_taken_name_are_refused_not_overwritten`

## Cas limites
- La route et le détail `by`/`at` du conflit ne sont pas dans ce ticket.

## Règles liées
- BR-EDIT-035, BR-EDIT-036

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
