---
id: BR-RESIL-007
domaine: RESIL
titre: Hors « Connecté », les dernières données restent affichées, marquées périmées
statut: à venir
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-007)
maj: 2026-10-05
---

# BR-RESIL-007 — Hors « Connecté », les dernières données restent affichées, marquées périmées

## Règle
Les vues gardent les dernières données connues avec leur âge (« Données d'il y a 2 minutes »). La bibliothèque conserve la dernière vue connue et sa date (`SnapshotStore`, `LinkManager::last_known`) ; le marquage visuel est de l'interface.

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
