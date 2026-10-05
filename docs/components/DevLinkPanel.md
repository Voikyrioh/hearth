# DevLinkPanel

Organisme · `apps/desktop/src/components/organisms/DevLinkPanel.vue`

Panneau de développement qui pilote le pont simulé (état de chaque serveur, issues d'opération). Visible seulement en mode développement ET dans un navigateur ; `?nodev` le masque.

- Props : aucune
- Événements et slots : aucun
- Notes : Importé par `App.vue` derrière `import.meta.env.DEV` : absent du binaire livré (vérifié : ni le panneau ni le pont simulé n'apparaissent dans `dist/`). Couvert par `e2e/shell.spec.ts`.
