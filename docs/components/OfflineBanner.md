# OfflineBanner

Organisme · `apps/desktop/src/components/organisms/OfflineBanner.vue`

Bandeau « Hors ligne » : « Serveur hors ligne. Dernier contact à 14h23. Nouvelle tentative automatique en cours. » et bouton « Réessayer maintenant ». `role="status"`, jamais modal. BR-RESIL-004. Quand les tentatives sont suspendues (`blocked`), il dit pourquoi : identité du serveur changée (bouton « Voir l'alerte » au lieu de « Réessayer maintenant »), agent ou client trop ancien (BR-CONN-003, 014).

- Props : `lastContactAt` (sans lui, la phrase omet l'heure), `blocked`
- Événements et slots : événements `retry`, `alert`
- Notes : Affiché par `ServerLayout` seulement dans l'état « Hors ligne ». Tests : `organisms.test.ts`, `shell.test.ts`.
