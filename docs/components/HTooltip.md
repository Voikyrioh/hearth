# HTooltip

Atome · `apps/desktop/src/components/atoms/HTooltip.vue`

Bulle d'explication au survol et au focus clavier (`role="tooltip"`), fermée par Échap. Le slot reçoit `describedby` à poser sur le contrôle. L'enveloppe existe même sans texte (le contrôle n'est jamais recréé quand l'explication apparaît : le focus clavier reste).

- Props : `text` (sans texte, pas de bulle), `placement` (`start`, `center`, `end`), `side` (`bottom`, `top`)
- Événements et slots : slot par défaut (`{ describedby }`)
- Notes : C'est l'unique affichage de la raison d'un blocage (lien, rôle, `hint`) : `HButton`, `HToggle` et `HInput` l'utilisent. Tests : `atoms.test.ts`.
