---
id: FIX-01M4ECZK55R0HTKS7SQH4WZ1ZZ
titre: Après un échec de lecture, « Réessayer » relançait l'ancien filtre, et rien ne disait que la recherche tapée n'était pas appliquée (HRT-43)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4ECZK55R0HTKS7SQH4WZ1ZZ : Après un échec de lecture, « Réessayer » relançait l'ancien filtre, et rien ne disait que la recherche tapée n'était pas appliquée (HRT-43)

## Symptôme
La recherche tapée échoue : la liste reste celle d'avant sans le dire ; « Réessayer » relisait le filtre appliqué, pas la saisie.

## Reproduction
Vitest `pages/audit.test.ts` « après un échec de lecture, dit que la recherche tapée n'est pas appliquée ». Rouge avant : message sans mention, relance avec l'ancien filtre.

## Cause root
`audit.retry()` relit toujours le filtre appliqué.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Le message dit « Ta recherche n'est pas appliquée. … » quand le brouillon diffère du filtre appliqué, et « Réessayer » relance le filtre tapé. `// FIX:01M4ECZK55R0HTKS7SQH4WZ1ZZ`.

## Règles
- Aucune règle métier modifiée.

## Non-régression
- Les tests ci-dessus.

## Références
- Tickets : HRT-39, HRT-40, HRT-41, HRT-43 (revue UX du 2026-10-08)
- Code : `pages/Audit.vue`
