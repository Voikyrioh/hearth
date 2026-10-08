---
id: FIX-01M4DNJ97KCR694M9JT5B7R67B
titre: Journal : textes bruts, raison en minuscules, lignes vides, horodatage ISO (C32)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DNJ97KCR694M9JT5B7R67B : Journal : textes bruts, raison en minuscules, lignes vides, horodatage ISO (C32)

## Symptôme
Raison « lecture seule » sans majuscule ; détail d'une entrée avec « Date source (UTC) 2026-10-08T05:24:48.113Z » et des lignes « Cible » et « Raison » vides ; aide de confirmation plus courte que la spec.

## Reproduction
`e2e/hrt43-44.spec.ts` « journal : textes lisibles » (majuscule de la raison, aucune valeur vide, aucun horodatage ISO) ; `e2e/reauth.spec.ts` (aide complète). Rouge avant : minuscule, ligne vide, ISO.

## Cause root
Les valeurs étaient affichées telles que l'agent les donne.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
`capitalize` et `sourceOf` dans `audit/format.ts` ; le détail retire les lignes vides ; aide `reauth.help` : phrase complète de la spec. `// FIX:01M4DNJ97KCR694M9JT5B7R67B`.

## Règles
- Aucune règle métier.

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-43 / HRT-44 (revue UX du 2026-10-08)
- Code : `components/molecules/AuditDetailDialog.vue`
