---
id: FIX-01M4DNJ3W899Y50NPSRQ8W8ZVN
titre: L'en-tête « Sessions ouvertes » était en chasse fixe, les autres non (C26)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DNJ3W899Y50NPSRQ8W8ZVN : L'en-tête « Sessions ouvertes » était en chasse fixe, les autres non (C26)

## Symptôme
Dans l'en-tête du tableau des comptes, « Sessions ouvertes » s'affichait dans la police mono des chiffres, pas les autres titres.

## Reproduction
`e2e/hrt43-44.spec.ts` « comptes à … l'en-tête n'a qu'une police » : une seule famille de police sur toutes les cellules d'en-tête, aux 5 tailles. Rouge avant : deux familles.

## Cause root
La classe `.table__num` (chiffres en mono, alignés à droite) était posée aussi sur la cellule d'en-tête.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
`.table thead .table__num { font-family: inherit; }` : l'en-tête garde l'alignement à droite, pas la chasse fixe. `// FIX:01M4DNJ3W899Y50NPSRQ8W8ZVN`.

## Règles
- Aucune règle métier.

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-43 / HRT-44 (revue UX du 2026-10-08)
- Code : `components/organisms/AccountTable.vue`
