---
id: BR-RESIL-020
domaine: RESIL
titre: Chaque serveur enregistré a son propre état de lien, indépendant des autres
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-020)
maj: 2026-10-05
---

# BR-RESIL-020 — Chaque serveur enregistré a son propre état de lien, indépendant des autres

## Règle
Le gestionnaire lance une tâche supervisée par serveur, avec sa propre machine à états, son flux, ses opérations en suspens et ses dates. Un serveur qui tombe, se bloque ou perd sa session ne change ni l'état ni les tentatives des autres ; `states()` les rend tous, `state(id)` un seul.

## Application (code)
- `crates/hearth-link/src/manager/mod.rs::LinkManager::{state, states}`, `Registry` ; `crates/hearth-link/src/manager/task.rs` (une tâche par serveur, `Runner`).

## Vérification
- Tests : `crates/hearth-link/tests/pinning.rs::each_server_has_its_own_independent_link` ; une panique dans la tâche d'un serveur ne touche que lui (`task_restarts`).

## Cas limites
- Les veilleurs de réveil et de réseau, eux, concernent tous les serveurs (le poste est le même) : chaque serveur retente de son côté.

## Règles liées
- BR-RESIL-006.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
