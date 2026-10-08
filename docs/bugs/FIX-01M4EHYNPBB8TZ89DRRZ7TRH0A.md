---
id: FIX-01M4EHYNPBB8TZ89DRRZ7TRH0A
titre: Les bandeaux de sécurité étaient bornés à 880 px au-dessus d'une page plus large, boutons empilés contre les bords (D2)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4EHYNPBB8TZ89DRRZ7TRH0A : Les bandeaux de sécurité étaient bornés à 880 px au-dessus d'une page plus large, boutons empilés contre les bords (D2)

## Symptôme
À 1366 « Activer le mode attaque » et « Plus d'infos » empilés touchaient le haut et le bas du bandeau ; à 1920 le bandeau de 880 px surmontait une grille de 1 600 px.

## Reproduction
`e2e/hrtp2.spec.ts` « bandeau de sécurité … à la largeur du contenu, actions sur une ligne avec de l'air » aux 5 tailles : largeur égale à celle du contenu, boutons sur une ligne, au moins 4 px d'air en haut et en bas. Rouge avant : boutons sur deux lignes. `e2e/hrt39-security.spec.ts` mis à jour (même largeur que le contenu).

## Cause root
La borne de 880 px de C35 datait d'avant les bornes de page de HRT-42 ; boutons autorisés à passer à la ligne dans un bandeau de 36 px.

## Impacté
L'interface du client (seconde passe UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Plus de borne propre au bandeau (il suit le contenu), boutons sans retour à la ligne, 4 px d'air. Les deux bandeaux cumulés passent de 80 à 82 px ; plafond du test de `layout.spec.ts` porté à 84 px, pour cette raison seulement. `// FIX:01M4EHYNPBB8TZ89DRRZ7TRH0A`.

## Règles
- Aucune règle métier modifiée.

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-39 (seconde passe UX : `contexts/hearth/art/ux-review-2026-10-08-passe2.md`)
- Code : `components/molecules/SecurityBanner.vue`
