---
id: BR-RESIL-016
domaine: RESIL
titre: L'icône de la zone de notification reflète l'état du lien
statut: à venir
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-016)
maj: 2026-10-05
---

# BR-RESIL-016 — L'icône de la zone de notification reflète l'état du lien

## Règle
L'icône change en temps réel avec l'état du lien du serveur affiché. Elle s'abonne aux événements d'état de la bibliothèque ; l'affichage est de la coquille Tauri.

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
