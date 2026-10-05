# HIcon

Atome · `apps/desktop/src/components/atoms/HIcon.vue`

Icône SVG en trait (24 px de grille, `currentColor`). Sans `label` : `aria-hidden`. Avec `label` : `role="img"`.

- Props : `name` (`plus`, `settings`), `label` (optionnel)
- Événements et slots : aucun
- Notes : Ajouter une icône = un cas de plus dans `IconName` et le gabarit. Tests : `HLogo.test.ts`.
