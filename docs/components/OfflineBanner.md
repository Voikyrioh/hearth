# OfflineBanner

Organisme · `apps/desktop/src/components/organisms/OfflineBanner.vue`

Bandeau « Hors ligne » : « Serveur hors ligne. Dernier contact à 14h23. Nouvelle tentative automatique en cours. » et bouton « Réessayer maintenant ». `role="status"`, jamais modal. BR-RESIL-004.

- Props : `lastContactAt` (sans lui, la phrase omet l'heure)
- Événements et slots : événement `retry`
- Notes : Affiché par `ServerLayout` seulement dans l'état « Hors ligne ». Tests : `organisms.test.ts`, `shell.test.ts`.
