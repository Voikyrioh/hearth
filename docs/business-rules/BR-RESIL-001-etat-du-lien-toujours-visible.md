---
id: BR-RESIL-001
domaine: RESIL
titre: L'état du lien est toujours visible dans l'en-tête
statut: à venir
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-001)
maj: 2026-10-05
---

# BR-RESIL-001 — L'état du lien est toujours visible dans l'en-tête

## Règle
L'indicateur d'état du lien du serveur affiché est visible en permanence dans l'en-tête, quelle que soit la vue. La bibliothèque fournit l'état (`LinkState`) et l'événement d'état (`since`, `last_contact_at`, `next_retry_at`) ; l'indicateur lui-même est de l'interface.

Statut : à venir, HRT-12 (états hors ligne de l'interface).

## Application (code)
- Interface : à venir, HRT-12. Données fournies par `crates/hearth-link/src/domain/state.rs::LinkMachine::status` et par les événements du `LinkManager`.

## Vérification
- À venir, HRT-12 (tests Vitest et Playwright de l'interface).

## Cas limites
- Sans objet pour la bibliothèque.

## Règles liées
- BR-RESIL-002 à BR-RESIL-005.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
