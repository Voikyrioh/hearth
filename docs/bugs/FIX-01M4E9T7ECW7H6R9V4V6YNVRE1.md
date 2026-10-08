---
id: FIX-01M4E9T7ECW7H6R9V4V6YNVRE1
titre: Tableau de bord sans mesure : « Vu il y a… » posé sur une page qui n'a rien vu
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4E9T7ECW7H6R9V4V6YNVRE1 : Tableau de bord sans mesure : « Vu il y a… » posé sur une page qui n'a rien vu

## Symptôme
Serveur hors ligne et aucune mesure jamais reçue : le message « Aucune mesure pour l'instant » était accompagné d'une estampille « Vu il y a … ».

## Reproduction
`pages/Dashboard.test.ts` « shows no « Vu il y a » stamp on a dashboard that never received a measure ». Rouge avant : l'estampille était présente. `router/shell.test.ts` : le tableau de bord sans mesure n'est plus daté.

## Cause root
`StaleSurface` posait l'estampille pour toute page non connectée, sans savoir si la page avait reçu quelque chose.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
La page déclare « rien reçu » (`composables/pageSeen.ts`) tant qu'elle n'a pas de machine ; `StaleSurface` ne pose alors pas l'estampille. `// FIX:01M4E9T7ECW7H6R9V4V6YNVRE1`.

## Règles
- BR-DASH-010, BR-DASH-014 (formateurs d'unités uniques).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-41 (revue UX du 2026-10-08)
- Code : `components/molecules/StaleSurface.vue, composables/pageSeen.ts, pages/Dashboard.vue`
