---
id: FIX-01M4D4FQRZ2QTRT57HMM6TAPBE
titre: Le panneau de session expirée se posait en haut à gauche et repoussait la dernière vue (C45)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D4FQRZ2QTRT57HMM6TAPBE : Le panneau de session expirée se posait en haut à gauche et repoussait la dernière vue (C45)

## Symptôme
Carte de 420 px plaquée à gauche, tableau de bord repoussé de 400 px ; une seule sortie hors accès révoqué.

## Reproduction
`e2e/layout.spec.ts` « session expirée à … » (1366, 1920, 2560) : panneau centré à 2 px, première carte du tableau de bord non repoussée, bouton d'envoi et « Utiliser un autre compte » visibles ; rouge avant.

## Cause root
Le panneau était un bloc du flux, aligné à gauche, avant la vue ; « Utiliser un autre compte » n'existait que pour l'accès révoqué.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Panneau en surimpression (`position: absolute` dans la zone de contenu, centré), placé après la vue dans le DOM pour passer au-dessus ; « Utiliser un autre compte » reste réservé à l'accès révoqué (BR-RESIL-014) ; l'identifiant reste modifiable ; centré horizontalement ET verticalement ; premier dans le DOM (premier élément focalisable de la zone), au-dessus par `z-index` (`--z-tooltip`), fond opaque et ombre `--card-edge`. Non traité ici : le curseur dans « Mot de passe » (lot formulaires, HRT-33). `// FIX:01M4D4FQRZ2QTRT57HMM6TAPBE`.

## Règles
- Design : `contexts/hearth/conceptions/design-system-web.md` (fenêtre minimale 1 100 px, jetons existants).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-35 (revue UX du 2026-10-08)
- Code : `components/organisms/ReconnectPanel.vue`, `layouts/ServerLayout.vue`
