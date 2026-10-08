# ErasurePendingNote

Organisme · `apps/desktop/src/components/organisms/ErasurePendingNote.vue`

Note de la page Sécurité : l'agent dit (administrateur seulement, `GET /security`, champ `erasure_pending`) que l'effacement physique des anciennes empreintes de requêtes est en attente (point de contrôle retenu, disque plein au démarrage). Dit quoi faire : rien, l'agent le reprend tout seul à son prochain démarrage (ADR-0034). Un fait, rien de sensible.

- Props : aucune
- Événements et slots : aucun
- Notes : état lu par `stores/security.ts` (`erasurePending`), tenu côté Rust par `security/book.rs`. Tests : `pages/SecurityMode.test.ts`, `e2e/hrt39-security.spec.ts`, `security::book::tests`.
