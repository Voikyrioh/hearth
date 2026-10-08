---
id: FIX-01M4EHYPMVHR2TY39G3BQ7ZJZA
titre: « Mes serveurs » : la pastille d'état passait seule à la ligne (D8)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4EHYPMVHR2TY39G3BQ7ZJZA : « Mes serveurs » : la pastille d'état passait seule à la ligne (D8)

## Symptôme
Pour un serveur à plusieurs étiquettes, « Connecté » seul sur sa ligne ; en ligne pour les autres.

## Reproduction
`e2e/hrtp2.spec.ts` « mes serveurs … la pastille d'état est sur la ligne du nom » à 1100. Rouge avant.

## Cause root
La pastille venait après les étiquettes dans la rangée qui passe à la ligne.

## Impacté
L'interface du client (seconde passe UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
La pastille suit le nom ; ce sont les étiquettes qui passent à la ligne. `// FIX:01M4EHYPMVHR2TY39G3BQ7ZJZA`.

## Règles
- Aucune règle métier modifiée.

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-36 (seconde passe UX : `contexts/hearth/art/ux-review-2026-10-08-passe2.md`)
- Code : `components/organisms/ServerRow.vue`
