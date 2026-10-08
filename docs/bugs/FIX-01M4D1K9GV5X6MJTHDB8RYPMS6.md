---
id: FIX-01M4D1K9GV5X6MJTHDB8RYPMS6
titre: Un chargement des mesures qui ne finit pas se lisait comme une panne (C5)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D1K9GV5X6MJTHDB8RYPMS6 : Un chargement des mesures qui ne finit pas se lisait comme une panne (C5)

## Symptôme
Sous la pastille « Connecté », « Chargement des mesures » tournait sans fin, sans sortie.

## Reproduction
`pages/Dashboard.test.ts` « after 3 seconds without measures, says so and offers to try again ».

## Cause root
Le chargement n'avait ni délai ni action.

## Impacté
Le tableau de bord (HRT-11), jamais publié.

## Workaround
Aucun.

## Correction
Après 3 secondes le texte devient « Mesures en cours de chargement… » et un bouton « Réessayer » reprend l'abonnement aux mesures du serveur. `// FIX:01M4D1K9GV5X6MJTHDB8RYPMS6`.

## Règles
- Design : grille 12 puis 6 colonnes (`contexts/hearth/conceptions/2026-10-04-design-ecrans-socle.md`).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-34 (revue UX du 2026-10-08)
- Code : `apps/desktop/src/pages/Dashboard.vue`
