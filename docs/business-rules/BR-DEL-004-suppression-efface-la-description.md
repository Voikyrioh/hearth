---
id: BR-DEL-004
domaine: DEL
titre: Supprimer un service efface sa description et ses valeurs scellées
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-DEL-004), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-DEL-004 — Supprimer un service efface sa description et ses valeurs scellées

## Règle
`ServiceSpecs::forget` efface toutes les révisions d'un service et leurs scellés en une transaction (la base écrase les pages libres, `secure_delete`). `discard_before` efface les révisions antérieures à la courante quand l'ancien conteneur est supprimé, jamais la courante. La suppression elle-même est HRT-85.

## Application (code)
- `crates/hearth-agent/src/application/service_specs.rs::ServiceSpecs::{forget, discard_before}`
- `crates/hearth-agent/src/infrastructure/sqlite/specs_repo.rs::SqliteSpecRepo::{forget, discard_before}`

## Vérification
- `crates/hearth-agent/tests/service_spec_secrets.rs::forgetting_a_service_erases_its_sealed_values`
- `crates/hearth-agent/tests/service_spec_secrets.rs::a_description_is_numbered_and_the_previous_revision_stays_readable`

## Cas limites
- La copie d'export sans secret (`.hearth/service.json`) n'est pas dans ce ticket.

## Règles liées
- BR-EDIT-035, BR-EDIT-034

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
