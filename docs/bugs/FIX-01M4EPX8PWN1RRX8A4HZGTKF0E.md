---
id: FIX-01M4EPX8PWN1RRX8A4HZGTKF0E
titre: L'anneau du serveur sélectionné était rogné dans la barre des serveurs (S2)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4EPX8PWN1RRX8A4HZGTKF0E : L'anneau du serveur sélectionné était rogné dans la barre des serveurs (S2)

## Symptôme
L'anneau (4 px hors de l'avatar) était coupé en haut par la zone qui défile de la barre.

## Reproduction
`e2e/hrt47.spec.ts` « l'anneau du serveur sélectionné est entier » aux 5 tailles : la boîte de l'anneau tient dans la liste et la barre. Rouge avant : -3 px en haut.

## Cause root
La liste des serveurs (`overflow-y: auto`) n'avait aucune marge autour des avatars.

## Impacté
Le client, vu par Voiky lors de son premier smoke sur la forge (HRT-47).

## Workaround
Aucun.

## Correction
8 px de marge verticale dans la liste, `overflow-x: hidden`. `// FIX:01M4EPX8PWN1RRX8A4HZGTKF0E`.

## Règles
- Aucune règle métier modifiée (la liste des disques côté agent est S1a, autre tâche).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-47 ; captures `contexts/hearth/art/smoke-2026-10-08/`
- Code : `components/organisms/ServerRail.vue`
