---
id: BR-AUDIT-020
domaine: AUDIT
titre: Rechargement manuel quand le lien est coupé
statut: à venir (HRT-07, HRT-14)
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-020), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-020 — Rechargement manuel quand le lien est coupé

## Règle
Côté interface : aucune relance automatique ; un bouton de rechargement manuel. Côté agent : `GET /audit` est sans état et idempotent ; recharger rend la page la plus récente.

## Application (code)
- `Sans objet côté agent.`.

## Vérification
- Règle d'interface : vérifiée avec l'écran du journal (HRT-14).

## Cas limites
- Aucun cas limite propre à l'agent.

## Règles liées
- BR-AUDIT-011.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
