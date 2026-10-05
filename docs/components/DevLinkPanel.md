# DevLinkPanel

Organisme · `apps/desktop/src/components/organisms/DevLinkPanel.vue`

Panneau de développement qui pilote le pont simulé (état de chaque serveur, issues d'opération). Visible seulement en mode développement ET dans un navigateur ; `?nodev` le masque.

- Props : aucune
- Événements et slots : aucun
- Notes : Importé par `App.vue` derrière `import.meta.env.DEV` : absent du binaire livré (`npm run check:dist` en CI : ni le panneau, ni ses libellés, ni le pont simulé, ni les données d'exemple dans `dist/`). Couvert par `e2e/shell.spec.ts`.
