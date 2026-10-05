---
id: BR-RESIL-008
domaine: RESIL
titre: Hors « Connecté », les actions qui demandent le serveur sont désactivées
statut: à venir
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-008)
maj: 2026-10-05
---

# BR-RESIL-008 — Hors « Connecté », les actions qui demandent le serveur sont désactivées

## Règle
Les boutons d'action sont désactivés avec une explication au survol. La bibliothèque refuse d'ailleurs l'envoi hors « Connecté » (`LinkError::NotConnected`, voir BR-RESIL-009) ; la désactivation visuelle est de l'interface.

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
