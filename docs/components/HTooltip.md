# HTooltip

Atome · `apps/desktop/src/components/atoms/HTooltip.vue`

Bulle d'explication au survol et au focus clavier (`role="tooltip"`), fermée par Échap. Le slot reçoit `describedby` à poser sur le contrôle.

- Props : `text`
- Événements et slots : slot par défaut (`{ describedby }`)
- Notes : Les contrôles désactivés par le lien ou le rôle utilisent l'attribut `title` posé par `v-needs-link`, pas ce composant. Tests : `atoms.test.ts`.
