---
id: BR-RESIL-012
domaine: RESIL
titre: Une session expire après 30 jours sans activité ; le jeton n'est stocké que haché
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-012), technique-socle §3 et §7, HRT-04
maj: 2026-10-04
---

# BR-RESIL-012 — Session glissante et jeton

## Règle
Une session vit 30 jours après sa dernière activité : chaque requête authentifiée repousse l'expiration (au plus une écriture toutes les 5 minutes). Passé ce délai : `401 SESSION_EXPIRED`. Un jeton inconnu et non révoqué (session déjà purgée après expiration) reçoit aussi `SESSION_EXPIRED`.

Le jeton est 32 octets aléatoires du système, rendu au client en hexadécimal (64 caractères) une seule fois. L'agent n'en conserve que le SHA-256 et retrouve la session par cette empreinte (le jeton en clair n'est jamais comparé) ; `TokenHash` se compare en temps constant. Aucun jeton n'est journalisé ni affiché (`Debug` masqué).

## Application (code)
- `crates/hearth-agent/src/domain/sessions.rs::{expiry_from, check, renewed_expiry}`.
- `crates/hearth-agent/src/domain/session_token.rs::{SessionToken, TokenHash}`.
- `crates/hearth-agent/src/application/sessions.rs::SessionService::{login, authenticate}`.
- `crates/hearth-agent/src/infrastructure/random.rs::OsTokenGen` — hasard du système.
- La purge des sessions expirées est périodique : `application/maintenance.rs`.

## Vérification
- Tests : `domain::sessions::tests`, `domain::session_token::tests` ; `tests/sessions_use_cases.rs` (expiration glissante) ; `tests/sessions_https.rs`.

## Cas limites
- L'expiration est exclusive : une session dont l'expiration est exactement `maintenant` est expirée.
- Horloge qui recule : aucun renouvellement.

## Règles liées
- BR-RESIL-014, BR-ACCT-008 à BR-ACCT-011 (fermetures de sessions).

## Historique
- 2026-10-04 — création (HRT-04, session 2026-10-04-hearth-creation).
