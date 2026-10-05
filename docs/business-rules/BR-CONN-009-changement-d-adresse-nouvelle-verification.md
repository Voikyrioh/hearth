---
id: BR-CONN-009
domaine: CONN
titre: Modifier l'adresse d'un serveur enregistré impose une nouvelle vérification de l'empreinte
statut: à venir
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-009)
maj: 2026-10-05
---

# BR-CONN-009 — Modifier l'adresse d'un serveur enregistré impose une nouvelle vérification de l'empreinte

## Règle
Quand l'adresse (hôte ou port) d'un serveur enregistré change, l'empreinte doit être relue (`probe`) et confirmée de nouveau avant toute connexion ; les identifiants mémorisés sont conservés mais rien ne part tant que la nouvelle adresse n'est pas validée.

Statut : à venir. Hors du ticket HRT-07 : la bibliothèque n'a pas encore d'opération « changer l'adresse » (aujourd'hui : supprimer puis ajouter le serveur, ce qui refait `probe`, confirmation et connexion). À livrer avec l'écran d'édition d'un serveur.

## Application (code)
- À venir (opération de modification d'adresse de `LinkManager`).

## Vérification
- À venir.

## Cas limites
- Sans objet tant que l'opération n'existe pas.

## Règles liées
- BR-CONN-001, BR-CONN-002, BR-CONN-011.

## Historique
- 2026-10-05 — création, à venir (HRT-07, review Stephen round 1).
