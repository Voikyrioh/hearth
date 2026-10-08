---
id: FIX-01M4D4FTZX89X3Z6MATEQ0KCN7
titre: Les actions d'un compte passaient sur deux lignes à 1280 et 1366 (C21)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D4FTZX89X3Z6MATEQ0KCN7 : Les actions d'un compte passaient sur deux lignes à 1280 et 1366 (C21)

## Symptôme
« Supprimer » seul sur sa ligne ou deux par deux, lignes de hauteur double.

## Reproduction
`e2e/layout.spec.ts` « comptes à … » : chaque bouton d'action entièrement dans la zone visible.

## Cause root
Les boutons avaient `flex-wrap: wrap` et les cellules passaient à la ligne.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Première correction (`nowrap`) REJETÉE en pré-revue : « Supprimer » sortait du tableau sans barre de défilement. Final : les cellules et les actions passent à la ligne proprement DANS le tableau, aucun bouton coupé (assertion : chaque bouton entièrement dans la zone visible aux cinq tailles ; rouge avant : bord droit 1451 px pour 1077-1343). « Une ligne par compte » n'est plus tenu à 1280 : la visibilité prime. `// FIX:01M4D4FTZX89X3Z6MATEQ0KCN7`.

## Règles
- Design : `contexts/hearth/conceptions/design-system-web.md` (fenêtre minimale 1 100 px, jetons existants).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-36 (revue UX du 2026-10-08)
- Code : `components/organisms/AccountTable.vue`
