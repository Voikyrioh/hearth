# HSpinner

Atome · `apps/desktop/src/components/atoms/HSpinner.vue`

Indicateur d'attente (arc qui tourne). Décoratif sans `label` ; `role="status"` avec. Mouvement réduit : rotation coupée (`base.css`), l'arc reste visible.

- Props : `label` (optionnel)
- Événements et slots : aucun
- Notes : Utilisé par `HButton` (état occupé). Tests : `atoms.test.ts`.
