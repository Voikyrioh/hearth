---
id: BR-RESIL-011
domaine: RESIL
titre: Aucune fenêtre d'erreur bloquante lors d'une perte de lien
statut: à venir
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-011)
maj: 2026-10-05
---

# BR-RESIL-011 — Aucune fenêtre d'erreur bloquante lors d'une perte de lien

## Règle
Les erreurs de lien sont notifiées discrètement. La bibliothèque ne remonte jamais d'erreur bloquante ni de panique : états et événements typés seulement ; l'affichage discret est de l'interface.

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
