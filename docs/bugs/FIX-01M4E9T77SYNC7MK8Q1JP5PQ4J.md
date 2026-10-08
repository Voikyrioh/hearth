---
id: FIX-01M4E9T77SYNC7MK8Q1JP5PQ4J
titre: Une jauge normale (22 %) portait la couleur d'une alerte : le rose du dégradé (C13)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4E9T77SYNC7MK8Q1JP5PQ4J : Une jauge normale (22 %) portait la couleur d'une alerte : le rose du dégradé (C13)

## Symptôme
La mémoire à 22 % était tracée dans un dégradé qui finit en rose, associé à une alerte.

## Reproduction
`e2e/hrt41.spec.ts` « légende ... jauges » : la jauge de la mémoire à 22 % porte le jeton braise `--ac` (rgb 255, 123, 61) ; épinglée à l'attention puis au critique (valeurs de BR-DASH-003), elle porte `--warn` puis `--crit`. Rouge avant : le trait était un dégradé (`url(...)`) qui finissait en `--ac2`, le rose.

## Cause root
Le niveau normal dessinait l'arc en dégradé de `--ac2` (rose, proche de `--crit`) vers `--ac` : un jeton d'alerte-like utilisé à tort sur une mesure normale.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Niveau normal : trait braise uni `--ac`, la couleur des courbes (identité braise du design system, inchangée) ; `--warn` et `--crit` n'apparaissent qu'aux seuils de BR-DASH-003. Les barres `HMeter` étaient déjà en `--ac`. `// FIX:01M4E9T77SYNC7MK8Q1JP5PQ4J`.

## Règles
- BR-DASH-010, BR-DASH-014 (formateurs d'unités uniques).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-41 (revue UX du 2026-10-08)
- Code : `components/atoms/HGaugeArc.vue, components/atoms/HMeter.vue`
