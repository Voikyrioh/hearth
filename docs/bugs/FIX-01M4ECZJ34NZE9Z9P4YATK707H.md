---
id: FIX-01M4ECZJ34NZE9Z9P4YATK707H
titre: Une courbe qui a le focus récitait une valeur par seconde (suivi de la revue de #58)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4ECZJ34NZE9Z9P4YATK707H : Une courbe qui a le focus récitait une valeur par seconde (suivi de la revue de #58)

## Symptôme
Sans repère posé, le texte de valeur d'une courbe focalisée était la dernière valeur, qui change à chaque mesure : un lecteur d'écran l'aurait annoncée chaque seconde.

## Reproduction
Vitest `components/dashboard.test.ts` « a focused chart does not speak every second ». Rouge avant : `aria-valuetext` suivait la dernière valeur.

## Cause root
`aria-valuetext` retombait sur la dernière valeur quand aucun repère n'était posé.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Sans repère posé, une consigne fixe (« Flèches pour parcourir les valeurs », jamais un nombre brut ni une valeur qui change ; plus d'`aria-valuenow`) ; le texte posé reste figé quand la fenêtre glisse. Les séparateurs de milliers du formateur sont écrits en échappement (`\u202F`, `\u00A0`), gardés par un test de la source. `// FIX:01M4ECZJ34NZE9Z9P4YATK707H`.

## Règles
- Aucune règle métier modifiée.

## Non-régression
- Les tests ci-dessus.

## Références
- Tickets : HRT-39, HRT-40, HRT-41, HRT-43 (revue UX du 2026-10-08)
- Code : `components/atoms/HAreaChart.vue, dashboard/format.ts`
