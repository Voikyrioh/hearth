---
id: FIX-01M4D4FP65X3TZW6JQAZ5YK7EX
titre: Les écrans vides étaient plaqués à gauche, titre cassé, deux tiers de l'écran vides (C27)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D4FP65X3TZW6JQAZ5YK7EX : Les écrans vides étaient plaqués à gauche, titre cassé, deux tiers de l'écran vides (C27)

## Symptôme
Journal vide, aucun résultat, « Aucune mesure » : bloc de 420 px collé au bord gauche, titre sur deux lignes.

## Reproduction
`e2e/layout.spec.ts` « journal vide à … » aux 5 tailles : écart gauche/droite à 2 px près, titre sur une ligne ; rouge avant (écart 360 à 1820 px).

## Cause root
`EmptyState` avait une largeur maximale de 420 px sans marges automatiques.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Centré horizontalement (`margin: auto`) ET verticalement (la zone de contenu, la surface périmée et la page du journal sont des colonnes qui s'étirent), largeur du panneau (`--panel-max`). Accueil (premier lancement) vérifié aux 5 tailles : inchangé, centré à 2 px. `// FIX:01M4D4FP65X3TZW6JQAZ5YK7EX`.

## Règles
- Design : `contexts/hearth/conceptions/design-system-web.md` (fenêtre minimale 1 100 px, jetons existants).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-35 (revue UX du 2026-10-08)
- Code : `components/molecules/EmptyState.vue`
