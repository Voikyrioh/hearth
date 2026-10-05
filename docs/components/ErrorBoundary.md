# ErrorBoundary

Molécule · `apps/desktop/src/components/molecules/ErrorBoundary.vue`

Frontière d'erreur du CONTENU d'une page : une page dont le RENDU plante (setup, render, cycle de vie) est remplacée par « Cette page a rencontré un problème » et « Réessayer ». La coquille autour reste utilisable. L'erreur est notifiée discrètement et journalisée (`reportUiError`).

- Props : `resetKey` (la route : un changement de page relance l'affichage)
- Événements et slots : slot par défaut
- Notes : Une erreur de gestionnaire d'événement ou une promesse rejetée ne déclenche pas le repli : notification discrète et journal seulement. Jamais autour de la coquille : `App.vue` en pose une autour des pages sans gabarit (Welcome, Settings) ; `ServerLayout` en pose une sous l'en-tête. Tests : `molecules.test.ts`.
