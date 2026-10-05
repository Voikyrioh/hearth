# ErrorBoundary

Molécule · `apps/desktop/src/components/molecules/ErrorBoundary.vue`

Frontière d'erreur : une page qui plante est remplacée par « Cette page a rencontré un problème » et « Réessayer ». La coquille autour reste utilisable. L'erreur est notifiée discrètement et journalisée (`reportUiError`).

- Props : `resetKey` (la route : un changement de page relance l'affichage)
- Événements et slots : slot par défaut
- Notes : Entoure `RouterView` dans `App.vue` et dans `ServerLayout`. Tests : `molecules.test.ts`.
