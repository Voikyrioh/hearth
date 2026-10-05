# DevActionPanel

Organisme · `apps/desktop/src/components/organisms/DevActionPanel.vue`

Panneau de développement (mode `vite` seulement, absent de `dist/`) : un bouton « Lancer une action » qui passe par `useServerAction` et la prop `needs-link`, comme le fera tout vrai bouton d'administration. Le comportement de l'action (réponse, ou lien coupé avant la réponse) se pilote par `__hearthSim.actionMode`. Sert aux scénarios Playwright de HRT-12.

- Props : aucune
- Événements et slots : aucun
- Notes : chargé par `App.vue` derrière `import.meta.env.DEV`, masqué par `?nodev`. Tests : `e2e/offline.spec.ts`.
