---
id: FIX-01M5PYVP4WFPG3YMVV68T041R4
titre: Le récapitulatif « Vérifiez avant de lancer » s'ouvrait défilé tout en bas (HRT-69, HRT-102)
date_découverte: 2026-10-24
date_correction: 2026-10-24
---

# FIX-01M5PYVP4WFPG3YMVV68T041R4 : Le récapitulatif « Vérifiez avant de lancer » s'ouvrait défilé tout en bas (HRT-69, HRT-102)

## Symptôme
En ouvrant le récapitulatif d'un déploiement ou d'une installation, la fenêtre est défilée sur le champ « Votre mot de passe » : le nom, l'image, les ports et les dossiers sont au-dessus, hors de vue.

## Cause root
`AdminActDialog` met le curseur dans le champ du mot de passe quand la fenêtre n'a pas de champ à elle ; `focus()` fait défiler la zone vers le champ, tout en bas d'un récapitulatif long.

## Reproduction
Installer Terraria depuis le Catalogue, arriver au récapitulatif : le défilement n'est pas à 0. Test : `apps/desktop/e2e/ux-2026-10-24.spec.ts` (défilement lu une fois le curseur posé).

## Impacté
`components/organisms/AdminActDialog.vue`, `components/molecules/ReauthField.vue`, `components/molecules/FormDialog.vue`.

## Workaround
Remonter à la main.

## Correction
À l'ouverture, le curseur est posé avec `{ preventScroll: true }` et la zone qui défile est remise en haut. Après un mot de passe refusé, `focus()` sans option : le champ revient sous les yeux (`ReauthField.focus(options?)`).

## Références
- Revue UX de Nora du 24/10, constat 15 ; tickets : HRT-69, HRT-102
- Code : `AdminActDialog.vue`, `FormDialog.vue` (marqueur `FIX:01M5PYVP4WFPG3YMVV68T041R4`)
