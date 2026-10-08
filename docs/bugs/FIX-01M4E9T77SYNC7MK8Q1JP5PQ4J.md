---
id: FIX-01M4E9T77SYNC7MK8Q1JP5PQ4J
titre: Les jauges à l'état normal étaient dessinées en dégradé rose-braise, la couleur d'une alerte (C13)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4E9T77SYNC7MK8Q1JP5PQ4J : Les jauges à l'état normal étaient dessinées en dégradé rose-braise, la couleur d'une alerte (C13)

## Symptôme
La mémoire à 22 % était tracée dans un dégradé qui finit en rose, associé à une alerte.

## Reproduction
`e2e/hrt41.spec.ts` « jauges neutres » : le trait d'une jauge normale n'est ni un dégradé ni le rose. Vitest `components/dashboard.test.ts` (pas de `linearGradient` au niveau normal). Rouge avant : dégradé `ac2` vers `ac`.

## Cause root
Le niveau normal utilisait le dégradé de marque de la maquette.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Niveau normal : trait turquoise neutre (`--cool`), barres `HMeter` de même ; l'attention et le critique gardent l'ambre et le rouge. Décision de Claude du 2026-10-08, à confirmer par Voiky (le design system disait « braise = état normal »). `// FIX:01M4E9T77SYNC7MK8Q1JP5PQ4J`.

## Règles
- BR-DASH-010, BR-DASH-014 (formateurs d'unités uniques).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-41 (revue UX du 2026-10-08)
- Code : `components/atoms/HGaugeArc.vue, components/atoms/HMeter.vue`
