---
id: FIX-01M4DKQV9ZT6XQF6P7ER02HFYZ
titre: Les boutons d'une fenêtre de dialogue sortaient de l'écran à 1100×680 (C20)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DKQV9ZT6XQF6P7ER02HFYZ : Les boutons d'une fenêtre de dialogue sortaient de l'écran à 1100×680 (C20)

## Symptôme
« Annuler » et « Créer » coupés en bas de la fenêtre de création de compte.

## Reproduction
`e2e/layout.spec.ts` « fenêtre de création de compte à … » aux 5 tailles et à 1100×420 : boutons entièrement dans la fenêtre, contenu au-dessus des boutons, contenu qui défile. Rouge avant : « Annuler dans la fenêtre » faux à 1100×680 et 1100×420.

## Cause root
Le dialogue n'avait pas de hauteur maximale : champs et boutons dépassaient.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Hauteur maximale de la fenêtre (`100dvh` moins les marges), colonne flex : le titre et les boutons fixes, une zone `dialog__scroll` qui défile entre eux. `// FIX:01M4DKQV9ZT6XQF6P7ER02HFYZ`.

## Règles
- Design : fenêtre minimale 1 100 px (`contexts/hearth/conceptions/design-system-web.md`).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-36 (revue UX du 2026-10-08)
- Code : `components/molecules/FormDialog.vue`
