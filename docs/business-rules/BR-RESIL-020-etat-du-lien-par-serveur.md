---
id: BR-RESIL-020
domaine: RESIL
titre: Chaque serveur a son propre état de lien, indépendant
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-020), HRT-09
maj: 2026-10-05
---

# BR-RESIL-020 — Chaque serveur a son propre état de lien, indépendant

## Règle
L'interface garde un état par serveur : la barre des serveurs montre sur chaque avatar l'état de SON serveur ; l'en-tête, le bandeau, les données périmées et les actions désactivées suivent le seul serveur affiché ; changer de serveur ne modifie aucun état.

## Application (code)
- `apps/desktop/src/stores/link.ts` (`events` par `serverId`, `stateOf`) ; `apps/desktop/src/stores/servers.ts` (`currentId`) ; `apps/desktop/src/components/organisms/ServerRail.vue` ; `apps/desktop/src/composables/useCurrentServer.ts`.

## Vérification
- Tests : `link-stores.test.ts::tracks the 5 states per server, independently`, `organisms.test.ts::shows each server's own link state in its avatar name`, `shell.test.ts::keeps each server's state independent when switching`, `needsLink.test.ts::only follows the CURRENT server`, `e2e/shell.spec.ts`.

## Cas limites
- Serveur supprimé pendant qu'il est affiché : retour au premier serveur restant, sinon à l'accueil (`router/index.ts::redirectFor`).

## Règles liées
- BR-RESIL-001, BR-RESIL-008.

## Historique
- 2026-10-05 — création (HRT-09, session 2026-10-04-hearth-creation). Portée par l'interface ; le calcul des états est dans `hearth-link` (ADR-0007).
