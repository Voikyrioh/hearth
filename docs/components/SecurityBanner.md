# SecurityBanner

Molécule · `apps/desktop/src/components/molecules/SecurityBanner.vue`

Base commune des bandeaux de sécurité (HRT-26, conception design, écran B). Un ton, un pictogramme, un titre ÉCRIT (le sens ne repose jamais sur la couleur seule), un texte, des actions : `alert` (attaque probable : contour complet `--warn`, triangle), `attack` (mode attaque actif : filet gauche `--sec-edge` rose `--ac2`, bouclier), `suspended` (mode suspendu : filet gauche turquoise `--cool`, horloge). Seul `alert` est `role="alert"` (annoncé une fois à l'apparition) ; les autres sont `role="status"`. `stamp` : « Dernier état connu à {heure}. » quand le lien n'est pas « Connecté ». Aucune logique.

- Props : `tone`, `title`, `stamp`
- Événements et slots : slot par défaut (le texte), slot `actions`
- Notes : Tests : `pages/SecurityMode.test.ts`, `e2e/attack-mode.spec.ts`.

HRT-39 (C35, FIX-01M4DJZAPV5ECT10JJ5JWNM8A6) : largeur du contenu (`--column-max`), l'action suit le message ; « Plus d'infos » et « Voir la page Sécurité » sont des boutons à contour.
