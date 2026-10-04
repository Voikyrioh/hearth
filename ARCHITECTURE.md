# Architecture — Hearth

Stack : Rust (Cargo workspace + Tauri 2) · Vue 3 · SQLite · Tokio · axum · rustls
Style : Hexagonale (agent, link) · Atomic design (front)
Entrée : `crates/hearth-agent/src/main.rs`, `apps/desktop/src-tauri/main.rs`, `apps/desktop/src/main.ts`
Maj : 2026-10-04

## Vue d'ensemble

Hearth est un système de monitoring de machines avec authentification et journalisation. L'agent serveur (Rust statique, écoute HTTPS) expose les mesures système et l'historique via WebSocket. Le client (Tauri + Vue 3 pour Windows) se connecte après épinglage du certificat TLS, s'authentifie par compte, et affiche les mesures avec graphiques. Tout le réseau (épinglage, connexion, résilience, opérations) vit dans la bibliothèque `hearth-link` ; la coquille Tauri et le front Vue ne savent rien du réseau.

## Carte

```
crates/
├── hearth-proto/    → Types partagés : requêtes, réponses, événements, codes d'erreur, version
├── hearth-agent/    → Serveur : authentification, sessions, mesures machine, audit, mise à jour [ARCHITECTURE.md]
│   ├── domain/      → Règles métier : comptes, mots de passe, rôles, journal, compatibilité
│   ├── application/ → Cas d'usage : authentication, monitoring, account management
│   ├── infrastructure/ → Adaptateurs : SQLite, sondes système, TLS, fichiers, téléchargement
│   └── entrypoint/  → Routes HTTP, WebSocket, sous-commandes CLI
├── hearth-link/     → Bibliothèque cliente : épinglage, connexion, machine à états, reconnexion [ARCHITECTURE.md]
└── xtask/           → Tâches build : binaire agent statique en conteneur, empaquetage, manifeste

apps/
├── desktop/src-tauri/  → Coquille Tauri : fenêtre, zone notification, coffre, mises à jour, relais
├── desktop/src/        → Interface Vue 3 : affichage état, validation saisies, formatage, tous textes
└── deploy/             → Script installation une commande

docs/              → INDEX.md (adr, business-rules, open-api, components, bugs)
```

## Flux principaux

- **Première connexion** : Client → probe (TLS sans confiance) → `/hello` → empreinte cert → confirmation utilisateur → login (Argon2id) → jeton → WebSocket `/stream` (snapshot + mesures chaque seconde). Secrets au coffre Windows.
- **Action pendant coupure réseau** : Client envoie action avec `Idempotency-Key:ULID` → lien coupé → Client affiche « Reconnexion… » (après 3 s) → tentatives espacées (0,5 s → 30 s) → lien rétabli → vérification état opération → résultat ou relance.
- **Mise à jour agent** : Admin → POST `/agent/update` → télécharge, vérifie minisign → superviseur lance détaché → arrêt ancien, échange binaires, redémarrage → vérification 60 s → succès ou rollback.

## Conventions locales

- **Architecture** : Hexagonal pour `hearth-agent` et `hearth-link` ; `domain/` aucune I/O, `application/` orchestration, `infrastructure/` adaptateurs externes.
- **Front** : Atomic design (atoms → molecules → organisms → pages). Aucune logique réseau dans la vue ; tout par événements Tauri typés.
- **Erreurs API** : `{ error: { code, message, details } }`. Codes transverses : `401 UNAUTHENTICATED`, `401 SESSION_EXPIRED`, `401 SESSION_REVOKED`, `403 FORBIDDEN_ROLE`, `409 OPERATION_IN_PROGRESS`, `422 VALIDATION_ERROR`, `426 INCOMPATIBLE_VERSION`, `429 TOO_MANY_ATTEMPTS`, `500 INTERNAL_ERROR`.
- **Logs** : Structurés sur stdout (repris par journald), niveau réglable.
- **Règles métier** : Partagées `domain/` (aucune I/O), testées unitairement, documentées `docs/business-rules/`.

## Commandes

- Tests : `cargo test` (domain seul) + `cargo test --test '*' ` (intégration) + `npx vitest` (front composants) + `npx playwright test` (e2e navigateur).
- Lint : `cargo clippy -- -D warnings` + `cargo fmt --check` + `npx biome check`.
- Build agent : `cargo xtask agent` (conteneur alpine, cible musl).
- Build client : `cd apps/desktop && npm run tauri build` (Windows NSIS).
- Run local agent : `RUST_LOG=debug cargo run --bin hearth-agent` (mode dev, sondes sysinfo).

## Où chercher

| Je cherche… | Dossier / fichier |
|---|---|
| une règle métier | `docs/business-rules/INDEX.md` puis `crates/hearth-agent/src/domain/` |
| un endpoint API | `docs/open-api/INDEX.md` puis `crates/hearth-agent/src/entrypoint/http.rs` |
| une route WebSocket | `docs/open-api/INDEX.md` (sujet `/stream`) puis `crates/hearth-agent/src/entrypoint/ws.rs` |
| un composant Vue | `docs/components/INDEX.md` puis `apps/desktop/src/components/` |
| la résilience du lien | `crates/hearth-link/src/state_machine.rs` et `docs/adr/ADR-0007-machine-a-etats-du-lien.md` |
| un cas d'authentification | `docs/adr/ADR-0005-tls-epingle.md` + `crates/hearth-agent/src/domain/accounts.rs` |
