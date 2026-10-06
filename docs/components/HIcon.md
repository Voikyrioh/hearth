# HIcon

Atome · `apps/desktop/src/components/atoms/HIcon.vue`

Jeu d'icônes SVG en trait (grille 24, `currentColor`) : un nom = quelques tracés. Sans `label` : `aria-hidden`. Avec `label` : `role="img"`. Aucun symbole typographique (⚙, ⚠) dans l'interface.

- Props : `name` (`plus`, `settings`, `alert`, `info`, `check`, `close`, `refresh`, `server`, `eye`, `eye-off`, `chevron-down`, `chevron-right`, `arrow-up`, `download`), `label` (optionnel), `size` (`md` 20 px, `sm` 16 px)
- Événements et slots : aucun
- Notes : Ajouter une icône = une entrée de plus dans `ICONS`. Tests : `atoms.test.ts`.
