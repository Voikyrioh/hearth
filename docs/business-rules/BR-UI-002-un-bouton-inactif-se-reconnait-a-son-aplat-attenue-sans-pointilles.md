---
id: BR-UI-002
domaine: UI
titre: Un bouton inactif se reconnaît d'un coup d'œil à son aplat atténué, sans pointillés
statut: active
invariant: false
source: revue UX du 17/11/2026 (Nora, I2), HRT-141
maj: 2026-11-17
---

# BR-UI-002 — Un bouton inactif : un aplat atténué, jamais de pointillés

## Règle
Un bouton inactif (`aria-disabled="true"`) se reconnaît d'un coup d'œil, pour les quatre variantes (principal, danger, secondaire, discret) : fond de surface neutre atténué, texte discret dont le contraste reste d'au moins 4,5 pour 1 sur cet aplat, contour plein et transparent, ni ombre ni couleur d'accent, curseur « interdit », sans passer par l'opacité. Les pointillés sont proscrits : ils disent « déposez quelque chose ici » ou « emplacement vide ».

## Application (code)
- `apps/desktop/src/components/atoms/HButton.vue` (règle `.btn[aria-disabled="true"]`)

## Vérification
- `apps/desktop/e2e/hrt134-141-ux.spec.ts` (« un bouton inactif se reconnaît d'un coup d'œil et reste lisible, pour les quatre variantes »)

## Historique
- 2026-11-17 — révise HRT-141 : le contour en tirets devient un contour plein transparent (revue UX, constat I2).
