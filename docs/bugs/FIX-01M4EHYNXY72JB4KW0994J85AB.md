---
id: FIX-01M4EHYNXY72JB4KW0994J85AB
titre: Comptes : l'en-tête de page débordait du tableau borné à 1 400 px (D3)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4EHYNXY72JB4KW0994J85AB : Comptes : l'en-tête de page débordait du tableau borné à 1 400 px (D3)

## Symptôme
À 1920, la carte s'arrêtait à 1 695 px, « Ajouter un compte » et la pastille à 1 896 px.

## Reproduction
`e2e/hrtp2.spec.ts` « comptes … l'en-tête ne dépasse pas le bord du tableau » aux 5 tailles. Rouge avant à 1920 et 2560.

## Cause root
Le tableau était borné (`--table-max`) mais pas l'en-tête de page posé par le gabarit.

## Impacté
L'interface du client (seconde passe UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Sur la page Comptes le gabarit borne sa colonne à `--table-max`, en-tête compris. `// FIX:01M4EHYNXY72JB4KW0994J85AB`.

## Règles
- Aucune règle métier modifiée.

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-42 (seconde passe UX : `contexts/hearth/art/ux-review-2026-10-08-passe2.md`)
- Code : `layouts/ServerLayout.vue`
