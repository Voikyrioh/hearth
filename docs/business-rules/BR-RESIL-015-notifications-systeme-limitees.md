---
id: BR-RESIL-015
domaine: RESIL
titre: Notifications système optionnelles, une par minute et par serveur
statut: à venir
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-015)
maj: 2026-10-05
---

# BR-RESIL-015 — Notifications système optionnelles, une par minute et par serveur

## Règle
Les notifications Windows (passage « Hors ligne », retour « Connecté ») sont désactivables et limitées. Elles s'appuient sur l'événement d'état de la bibliothèque ; la limitation et le réglage sont de la coquille Tauri.

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
