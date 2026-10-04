# Architecture — Hearth

Stack : Rust (Cargo workspace + Tauri 2) · Vue 3 · SQLite · Tokio · axum · rustls
Style : Hexagonale (agent, link) · Atomic design (front)
Entrée : `crates/hearth-agent/src/main.rs` (→ `entrypoint/cli.rs`), `apps/desktop/src-tauri/main.rs`, `apps/desktop/src/main.ts`
Maj : 2026-10-04

## Vue d'ensemble

Hearth est un système de monitoring de machines avec authentification et journalisation. L'agent serveur (Rust statique, écoute HTTPS) expose les mesures système et l'historique via WebSocket. Le client (Tauri + Vue 3 pour Windows) se connecte après épinglage du certificat TLS, s'authentifie par compte, et affiche les mesures avec graphiques. Tout le réseau (épinglage, connexion, résilience, opérations) vit dans la bibliothèque `hearth-link` ; la coquille Tauri et le front Vue ne savent rien du réseau.

## Carte

```
crates/
├── hearth-proto/    → Types partagés, sans E/S ni framework : `api/` (corps de requêtes/réponses : `hello`, `sessions`, `accounts`, `operations`), `error` (ErrorBody, ErrorCode), `fingerprint` (empreinte SHA-256, affichage 8 × 4), `headers` (noms des en-têtes), `product` (nom, port 7341), `version`
├── hearth-agent/    → Serveur : identité TLS, routes `/hello`, sessions, comptes, opérations (règles, SQLite, Argon2id, garde de rôle, sous-commandes `account`) ; mesures, audit, mise à jour à venir [ARCHITECTURE.md]
│   ├── domain/      → Règles métier pures : politique de création de l'identité, identifiant d'installation, comptes (`accounts/`), sessions et jetons, verrouillage, compatibilité de versions, opérations, `Secret` (journal à venir)
│   ├── application/ → Cas d'usage (`hello`, `accounts`, `sessions`, `operations`, `maintenance`) et `ports/` (un port de lecture et un port d'écriture par sujet, `Store` + `UnitOfWork`, PasswordHasher, Clock, IdGen, TokenGen…)
│   ├── infrastructure/ → Adaptateurs : `tls/` (certificat auto-signé, config rustls TLS 1.3), `config/` (agent.toml + HEARTH_*), `logging.rs` (texte ou JSON), `system/` (nom, MAC), `sqlite/` (hearth.db, migrations, dépôts et unité de travail SQLx), `argon2.rs`, `data_dir.rs`, `random.rs` ; sondes à venir
│   ├── entrypoint/  → `http/` (axum, table `ENDPOINTS`, couche d'accès et suivi posés depuis la table, couche de version, erreurs, serveur HTTPS), `cli.rs` (options et sous-commandes dont `account …`), `account.rs`, `terminal.rs` (saisie du mot de passe), `tasks.rs` (purge périodique) et `signal.rs`
│   └── app.rs       → Racine de composition : charge la config, assemble adaptateurs, cas d'usage et serveur, exécute la commande
├── hearth-link/     → Bibliothèque cliente : épinglage, connexion, machine à états, reconnexion [ARCHITECTURE.md]
└── xtask/           → Tâches build : binaire agent statique en conteneur, empaquetage, manifeste

apps/
├── desktop/src-tauri/  → Coquille Tauri : fenêtre, zone notification, coffre, mises à jour, relais
├── desktop/src/        → Interface Vue 3 : affichage état, validation saisies, formatage, tous textes
└── deploy/             → Script installation une commande

docs/              → INDEX.md (adr, business-rules, open-api, components, bugs)
```

## Flux principaux

- **Première connexion** : Client → probe (TLS sans confiance) → `/hello` → empreinte cert → confirmation utilisateur → `POST /sessions` (`X-Hearth-Api`, Argon2id, verrouillage progressif) → jeton (haché en base, session glissante 30 jours) → WebSocket `/stream` (snapshot + mesures chaque seconde, à venir). Secrets au coffre Windows.
- **Action pendant coupure réseau** : Client envoie action avec `Idempotency-Key:ULID` → lien coupé → Client affiche « Reconnexion… » (après 3 s) → tentatives espacées (0,5 s → 30 s) → lien rétabli → vérification état opération → résultat ou relance.
- **Mise à jour agent** : Admin → POST `/agent/update` → télécharge, vérifie minisign → superviseur lance détaché → arrêt ancien, échange binaires, redémarrage → vérification 60 s → succès ou rollback.

## Conventions locales

- **Architecture** : Hexagonal pour `hearth-agent` et `hearth-link` ; `domain/` aucune I/O, `application/` orchestration, `infrastructure/` adaptateurs externes.
- **Front** : Atomic design (atoms → molecules → organisms → pages). Aucune logique réseau dans la vue ; tout par événements Tauri typés.
- **Erreurs API** : `{ error: { code, message, details } }`. Codes transverses : `401 UNAUTHENTICATED`, `401 INVALID_CREDENTIALS`, `401 SESSION_EXPIRED`, `401 SESSION_REVOKED`, `403 FORBIDDEN_ROLE`, `409 OPERATION_IN_PROGRESS`, `422 VALIDATION_ERROR`, `426 INCOMPATIBLE_VERSION`, `429 TOO_MANY_ATTEMPTS`, `500 INTERNAL_ERROR` ; comptes : `USERNAME_TAKEN`, `LAST_ADMIN`, `CONFLICT`, `WEAK_PASSWORD`, `WRONG_PASSWORD` ; `IDEMPOTENCY_KEY_REUSED` (422), `BUSY` (503).
- **Logs** : Structurés sur stdout (repris par journald), niveau réglable.
- **Règles métier** : Partagées `domain/` (aucune I/O), testées unitairement, documentées `docs/business-rules/`.

## Commandes

- Tests : `cargo test` (domain seul) + `cargo test --test '*' ` (intégration) + `npx vitest` (front composants) + `npx playwright test` (e2e navigateur).
- Lint : `cargo clippy -- -D warnings` + `cargo fmt --check` + `npx biome check`.
- Build agent : `cargo xtask agent` (conteneur alpine, cible musl).
- Build client : `cd apps/desktop && npm run tauri build` (Windows NSIS).
- Comptes sur le serveur (sans réseau) : `cargo run --bin hearth-agent -- --data-dir ./.dev-data account add marie --role admin` (mot de passe demandé sans écho, ou `HEARTH_ACCOUNT_PASSWORD`), puis `account list|passwd|role|remove|revoke` ; runbook `docs/runbooks/recuperer-acces-administrateur.md`. Régénérer `.sqlx/` après une requête ou migration modifiée : voir `CLAUDE.md`.
- Run local agent : `RUST_LOG=debug cargo run --bin hearth-agent -- serve --data-dir ./.dev-data` ; empreinte : `cargo run --bin hearth-agent -- fingerprint --data-dir ./.dev-data`. Configuration : `agent.toml` (`--config`, `HEARTH_CONFIG`) puis variables `HEARTH_PORT`, `HEARTH_LISTEN_ADDR`, `HEARTH_DATA_DIR`, `HEARTH_MANAGED`, puis `--data-dir`. Journaux : `HEARTH_LOG_FORMAT=json|text` (texte en terminal, JSON sinon), niveau par `RUST_LOG`. Dossier de données par défaut : `/var/lib/hearth` (Linux), `%LOCALAPPDATA%\hearth-agent` (Windows, mode dev).

## Où chercher

| Je cherche… | Dossier / fichier |
|---|---|
| une règle métier | `docs/business-rules/INDEX.md` puis `crates/hearth-agent/src/domain/` |
| un endpoint API | `docs/open-api/INDEX.md` puis `crates/hearth-agent/src/entrypoint/http/` |
| une route WebSocket | `docs/open-api/INDEX.md` (sujet `/stream`) puis `crates/hearth-agent/src/entrypoint/ws/` (à venir) |
| l'empreinte / le certificat de l'agent | `crates/hearth-proto/src/fingerprint.rs`, `domain/identity_policy.rs` et `infrastructure/tls/` |
| un composant Vue | `docs/components/INDEX.md` puis `apps/desktop/src/components/` |
| la résilience du lien | `crates/hearth-link/src/state_machine.rs` et `docs/adr/ADR-0007-machine-a-etats-du-lien.md` |
| un cas d'authentification | `docs/adr/ADR-0005-tls-epingle.md`, `crates/hearth-agent/src/application/sessions.rs` et `domain/{sessions,lockout,session_token}.rs` |
| qui a le droit d'appeler une route | `crates/hearth-agent/src/entrypoint/http/mod.rs` (`ENDPOINTS`) et `auth.rs` (`guard`, `Caller`) |
| une table, une migration, une requête SQL | `crates/hearth-agent/migrations/` et `crates/hearth-agent/src/infrastructure/sqlite/` |
| une règle de compte (identifiant, mot de passe, dernier administrateur) | `docs/business-rules/BR-ACCT-*.md` puis `domain/accounts/` |
