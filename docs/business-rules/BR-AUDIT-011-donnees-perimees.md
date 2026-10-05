---
id: BR-AUDIT-011
domaine: AUDIT
titre: Données périmées quand le lien est coupé
statut: à venir (HRT-07, HRT-14)
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-011), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-011 — Données périmées quand le lien est coupé

## Règle
Côté interface et bibliothèque de liaison : les événements déjà chargés restent affichés, marqués périmés ; au retour du lien, la liste se met à jour avec les événements manqués. L'agent n'a rien de propre à faire : la page se recharge par curseur, sans état serveur.

## Application (code)
- `Sans objet côté agent : `GET /audit` est sans état.`.

## Vérification
- Règle d'interface : vérifiée avec l'écran du journal (HRT-14).

## Cas limites
- Règle de l'interface : fiche tenue ici pour mémoire.

## Règles liées
- BR-AUDIT-010, BR-AUDIT-020, BR-RESIL-008.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
