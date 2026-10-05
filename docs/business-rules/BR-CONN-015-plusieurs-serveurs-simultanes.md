---
id: BR-CONN-015
domaine: CONN
titre: Plusieurs serveurs peuvent être connectés en même temps ; basculer ne ferme rien
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-015)
maj: 2026-10-05
---

# BR-CONN-015 — Plusieurs serveurs peuvent être connectés en même temps ; basculer ne ferme rien

## Règle
Chaque serveur a sa tâche, son flux, son état de lien et ses opérations en suspens. La bibliothèque n'a pas de notion de « serveur courant » : l'interface choisit lequel afficher, les autres restent connectés (BR-RESIL-020).

## Application (code)
- `crates/hearth-link/src/manager/mod.rs::LinkManager::{states, servers}` ; `manager/task.rs` (une tâche par serveur).

## Interface (coquille et vue)
- `apps/desktop/src/components/organisms/ServerRow.vue` et `pages/Servers.vue` : un état de lien par serveur ; ouvrir ou se déconnecter d'un serveur ne touche pas aux autres. Tests : `src/pages/Servers.test.ts::lists every server with its own state…`, `::disconnects one server without touching the others…`.

## Vérification
- Tests : `crates/hearth-link/tests/pinning.rs::each_server_has_its_own_independent_link`.

## Cas limites
- Au plus 4 flux authentifiés par compte côté agent : l'excédent reçoit `BUSY` (tentative reportée).

## Règles liées
- BR-RESIL-020.

## Historique
- 2026-10-05 — création (HRT-07, review Stephen round 1).
- 2026-10-05 : section Interface (HRT-10).
