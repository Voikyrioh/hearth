---
id: FIX-01M4DKQWTWVNDXFPWJ5SE88SN7
titre: Les actions d'un compte passaient sur deux lignes à 1280 et 1366 (C21)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DKQWTWVNDXFPWJ5SE88SN7 : Les actions d'un compte passaient sur deux lignes à 1280 et 1366 (C21)

## Symptôme
« Supprimer » seul sur sa ligne ou deux par deux, lignes de hauteur double.

## Reproduction
`e2e/layout.spec.ts` « comptes à … » : une ligne par compte (au plus 76 px) à 1280 et plus, aucun bouton coupé ni hors du tableau aux 5 tailles. Rouge avant : lignes de 177 px (1100) et 97 px (1280, 1366).

## Cause root
Quatre boutons plus cinq colonnes ne tiennent pas dans 950 px.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Actions sans retour à la ligne ; sous 1100 px de tableau la colonne « Créé le » (la moins utile) est masquée et les marges resserrées ; à la fenêtre minimale (moins de 850 px de tableau) les actions repassent à la ligne dans leur cellule (jamais coupées). Pas de `nowrap` forcé : la tentative précédente coupait « Supprimer ». `// FIX:01M4DKQWTWVNDXFPWJ5SE88SN7`.

## Règles
- Design : fenêtre minimale 1 100 px (`contexts/hearth/conceptions/design-system-web.md`).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-36 (revue UX du 2026-10-08)
- Code : `components/organisms/AccountTable.vue`
