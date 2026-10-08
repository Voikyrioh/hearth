---
id: FIX-01M4DNJ3P75DN25714EBCH2EY2
titre: Le rôle en cours de changement ne se distinguait pas des autres cellules (C22)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DNJ3P75DN25714EBCH2EY2 : Le rôle en cours de changement ne se distinguait pas des autres cellules (C22)

## Symptôme
Dans le tableau des comptes, la liste de choix du rôle s'ouvrait sans aucun repère : même fond que les autres cellules.

## Reproduction
`e2e/hrt43-44.spec.ts` « comptes à … le rôle en cours de changement a un repère » : fond de la cellule différent de celui d'une autre cellule et trait visible, aux 5 tailles. Rouge avant : fond identique, ni ombre ni contour.

## Cause root
Aucun style sur `td[data-editing]`.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Cellule en cours de changement : fond `--card-2` et trait `--ac` de `--ring-width` (`td[data-editing]`, `AccountTable.vue`). `// FIX:01M4DNJ3P75DN25714EBCH2EY2`.

## Règles
- Aucune règle métier.

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-43 / HRT-44 (revue UX du 2026-10-08)
- Code : `components/organisms/AccountTable.vue`
