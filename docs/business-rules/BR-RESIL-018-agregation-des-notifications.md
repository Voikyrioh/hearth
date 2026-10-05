---
id: BR-RESIL-018
domaine: RESIL
titre: Coupures répétées : les notifications s'agrègent
statut: à venir
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-018)
maj: 2026-10-05
---

# BR-RESIL-018 — Coupures répétées : les notifications s'agrègent

## Règle
Une même erreur répétée affiche un compteur. La bibliothèque expose le nombre d'échecs consécutifs (`Status::failed_attempts`) ; l'agrégation est de l'interface.

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
