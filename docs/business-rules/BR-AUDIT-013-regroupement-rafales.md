---
id: BR-AUDIT-013
domaine: AUDIT
titre: Les refus en rafale sont regroupés à l'affichage
statut: à venir (HRT-14)
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-013), HRT-05
maj: 2026-10-04
---

# BR-AUDIT-013 — Les refus en rafale sont regroupés à l'affichage

## Règle
Côté interface : au moins 5 connexions refusées en 2 minutes depuis la même origine se regroupent sur la page chargée (« X tentatives refusées en Y min »). L'agent fournit ce qu'il faut : adresse de la connexion, action, résultat, date. Seuil fixé par la conception technique (§7).

## Application (code)
- `hearth-proto::api::audit::AuditEventItem` (champs `origin.addr`, `action`, `outcome`, `at`).

## Vérification
- Règle d'interface : vérifiée avec l'écran du journal (HRT-14).

## Cas limites
- Le regroupement ne porte pas sur le compte : l'identifiant saisi n'est pas retenu (BR-AUDIT-006).

## Règles liées
- BR-AUDIT-006.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
