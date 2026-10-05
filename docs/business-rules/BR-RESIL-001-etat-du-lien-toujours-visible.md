---
id: BR-RESIL-001
domaine: RESIL
titre: L'état du lien du serveur affiché est toujours visible dans l'en-tête
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-001), HRT-09
maj: 2026-10-05
---

# BR-RESIL-001 — L'état du lien du serveur affiché est toujours visible dans l'en-tête

## Règle
Dans toute vue d'un serveur, l'en-tête montre en permanence la pastille d'état du lien de CE serveur, avec le libellé exact « Connecté », « Reconnexion… », « Hors ligne », « Session expirée » ou « Accès révoqué ». La couleur n'est jamais seule à porter le sens. L'en-tête est hors de la frontière d'erreur d'une page : une page qui plante ne le retire pas. Chaque serveur a son état (BR-RESIL-020). La bibliothèque fournit l'état (`LinkState`) et l'événement d'état (`since`, `last_contact_at`, `next_retry_at`, `reason`) ; l'indicateur lui-même est de l'interface.

## Application (code)
- Bibliothèque :
  - `crates/hearth-link/src/domain/state.rs::LinkMachine::status` et les événements du `LinkManager`.
- Interface :
  - `apps/desktop/src/components/molecules/LinkStatePill.vue` (libellés, point de couleur, clignotement de la reconnexion coupé si mouvement réduit).
  - `apps/desktop/src/components/organisms/AppHeader.vue` (pastille de `serverId`) ; `apps/desktop/src/layouts/ServerLayout.vue` (en-tête hors de `ErrorBoundary`).
  - Types : `apps/desktop/src/link/types.ts::LinkState`.

## Vérification
- Interface : `molecules.test.ts::LinkStatePill`, `organisms.test.ts::AppHeader`, `shell.test.ts::shows rail, navigation, header with the link pill, and the page`, `e2e/shell.spec.ts` (« connecté, reconnexion, hors ligne puis retour », « mouvement réduit »).
- Bibliothèque : voir les tests des règles BR-RESIL-002 à BR-RESIL-005 (calcul des états).

## Cas limites
- Avant le premier événement d'un serveur, l'état affiché est « Reconnexion… » (jamais « Connecté » par défaut).
- Mouvement réduit : la pastille ne clignote pas (`prefers-reduced-motion`).

## Règles liées
- BR-RESIL-002 à BR-RESIL-005, BR-RESIL-012, BR-RESIL-014, BR-RESIL-020.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — précisé (HRT-07, review Stephen round 1).
- 2026-10-05 — création de la partie interface (HRT-09, session 2026-10-04-hearth-creation). Portée par l'interface ; le calcul des états est dans `hearth-link` (ADR-0007).
- 2026-10-05 — fiches HRT-07 et HRT-09 réunies (fusion de main dans feat/HRT-07-link).
