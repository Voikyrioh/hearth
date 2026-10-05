---
id: BR-RESIL-020
domaine: RESIL
titre: Chaque serveur enregistré a son propre état de lien, indépendant des autres
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-020), HRT-09
maj: 2026-10-05
---

# BR-RESIL-020 — Chaque serveur enregistré a son propre état de lien, indépendant des autres

## Règle
Le gestionnaire lance une tâche supervisée par serveur, avec sa propre machine à états, son flux, ses opérations en suspens et ses dates. Un serveur qui tombe, se bloque ou perd sa session ne change ni l'état ni les tentatives des autres ; `states()` les rend tous, `state(id)` un seul.

L'interface garde un état par serveur : la barre des serveurs montre sur chaque avatar l'état de SON serveur ; l'en-tête, le bandeau, les données périmées et les actions désactivées suivent le seul serveur affiché ; changer de serveur ne modifie aucun état.

## Application (code)
- Bibliothèque :
  - `crates/hearth-link/src/manager/mod.rs::LinkManager::{state, states}`, `Registry` ; `crates/hearth-link/src/manager/task.rs` (une tâche par serveur, `Runner`).
- Interface :
  - `apps/desktop/src/stores/link.ts` (`events` par `serverId`, `stateOf`) ; `apps/desktop/src/stores/servers.ts` (`currentId`) ; `apps/desktop/src/components/organisms/ServerRail.vue` ; `apps/desktop/src/composables/useCurrentServer.ts`.

## Vérification
- Bibliothèque : `crates/hearth-link/tests/pinning.rs::each_server_has_its_own_independent_link` ; une panique dans la tâche d'un serveur ne touche que lui (`task_restarts`).
- Interface : `link-stores.test.ts::tracks the 5 states per server, independently`, `organisms.test.ts::shows each server's own link state in its avatar name`, `shell.test.ts::keeps each server's state independent when switching`, `needsLink.test.ts::only follows the CURRENT server`, `e2e/shell.spec.ts`.
- Interface : `apps/desktop/src/stores/offline.test.ts``::la coupure de l'un ne touche pas l'autre`, `apps/desktop/e2e/offline.spec.ts` (« l'état de chaque serveur est indépendant »).
- Coquille : `apps/desktop/src-tauri/tests/alerts.rs::each_server_is_independent`, `presence.rs::each_server_has_its_own_window`.

## Cas limites
- Les veilleurs de réveil et de réseau, eux, concernent tous les serveurs (le poste est le même) : chaque serveur retente de son côté.
- Serveur supprimé pendant qu'il est affiché : retour au premier serveur restant, sinon à l'accueil (`router/index.ts::redirectFor`).

## Règles liées
- BR-RESIL-001, BR-RESIL-006, BR-RESIL-008.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — création de la partie interface (HRT-09, session 2026-10-04-hearth-creation). Portée par l'interface ; le calcul des états est dans `hearth-link` (ADR-0007).
- 2026-10-05 — fiches HRT-07 et HRT-09 réunies (fusion de main dans feat/HRT-07-link).
- 2026-10-05 : notifications et icône par serveur (HRT-12).
