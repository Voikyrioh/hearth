# OfflineBanner

Organisme · `apps/desktop/src/components/organisms/OfflineBanner.vue`

Bandeau « Hors ligne » : « Serveur hors ligne. Dernier contact à 14h23. Nouvelle tentative automatique en cours. » et bouton « Réessayer maintenant ». `role="status"`, jamais modal. BR-RESIL-004. Quand les tentatives sont suspendues (`blocked`), il dit pourquoi : identité du serveur changée (bouton « Voir l'alerte » au lieu de « Réessayer maintenant »), agent ou client trop ancien (BR-CONN-003, 014) : « Les versions du client et de l'agent ne sont pas compatibles. Mets à jour l'agent. » (ou « … Demande à un administrateur de mettre à jour l'agent. » pour le rôle Lecture seule, ou « … Mets à jour le client. »), avec, pour le client, le bouton « Mettre à jour le client » (ou « Chercher une mise à jour du client ») ; l'agent trop ancien ne peut pas être mis à jour depuis l'application (BR-UPDATE-020, 021, ADR-0021).

- Props : `lastContactAt` (sans lui, la phrase omet l'heure), `blocked`, `role`, `clientAction`, `clientBusy`
- Événements et slots : événements `retry`, `alert`, `client`
- Notes : Affiché par `ServerLayout` seulement dans l'état « Hors ligne ». Tests : `organisms.test.ts`, `shell.test.ts`.
