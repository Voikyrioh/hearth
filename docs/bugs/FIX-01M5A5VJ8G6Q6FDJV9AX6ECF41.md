---
id: FIX-01M5A5VJ8G6Q6FDJV9AX6ECF41
titre: Un cadre de focus blanc entourait le titre du détail après un clic ou une action (HRT-65)
date_découverte: 2026-10-19
date_correction: 2026-10-19
---

# FIX-01M5A5VJ8G6Q6FDJV9AX6ECF41 : Un cadre de focus blanc entourait le titre du détail après un clic ou une action (HRT-65)

## Symptôme
Après avoir fermé une confirmation ou ouvert un détail à la souris, le titre du service était encadré d'un rectangle blanc épais, comme un champ sélectionné.

## Reproduction
`apps/desktop/e2e/hrt65-lifecycle.spec.ts` (« le cadre de focus n'entoure pas le titre après un clic, seulement au clavier ») et `src/a11y/inputModality.test.ts`.

## Cause root
L'écran remet le focus au titre (`tabindex="-1"`) par le code ; la règle globale `:focus-visible` l'encadre alors que le dernier geste est à la souris.

## Impacté
Tout élément `tabindex="-1"` qui reçoit le focus par le code après un geste à la souris.

## Workaround
Aucun.

## Correction
`installInputModality` (`src/a11y/inputModality.ts`, installé dans `main.ts`) pose `data-input="pointer|keyboard"` sur la racine ; `base.css` retire le contour de `[tabindex="-1"]:focus-visible` quand le dernier geste est à la souris. Le repère reste au clavier.

## Références
- Ticket : HRT-65, PR #108 ; revue UX Nora, constat 7
