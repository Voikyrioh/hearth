---
id: ADR-0006
titre: Persistance agent : SQLx + SQLite
type: librairie
statut: acceptée
date: 2026-10-04
portee: projet
remplace: —
liens: [conception technique 2026-10-04, section 3]
---

# ADR-0006 — Persistance agent : SQLx + SQLite

## Contexte

Agent persiste : comptes (username, hash mot de passe, rôle), sessions (token, client_name, expiration), audit (action, acteur, résultat), opérations (clé idempotence, statut, résultat). Trois choix : fichier JSON plat (lenteur recherche/filtres), ORM lourd (Diesel, surcharge), **SQLx + SQLite** (vérifié à la compilation, léger, requêtes écrites manuellement conformes best-practices).

## Décision

Persistance en SQLite, fichier unique `/var/lib/hearth/hearth.db`. Requêtes via SQLx (async + vérification compile-time). Mode WAL (Write-Ahead Logging) pour concurrence. Migrations embarquées, appliquées au startup via `sqlx::migrate!()`.

## Versions et installation

- **SQLx** : 0.8+
- **SQLite** : 3.26+ (inclus dans musl)
- **Migration** : `sqlx-cli` pour dev local : `sqlx migrate run`

## Usage — exemple query

```rust
// Requête typée, vérifiée au compile via SQLX_OFFLINE
let account = sqlx::query_as::<_, Account>(
    "SELECT id, username, password_hash, role, created_at, password_changed_at FROM accounts WHERE id = $1"
)
.bind(account_id)
.fetch_one(&pool)
.await?;

// Insertion avec clé idempotence
sqlx::query(
    "INSERT OR IGNORE INTO operations (id, account_id, kind, status, created_at) 
     VALUES ($1, $2, $3, $4, $5)"
)
.bind(op_id)
.bind(account_id)
.bind("update_agent")
.bind("running")
.bind(now)
.execute(&pool)
.await?;
```

## Quand NE PAS l'appliquer / limites

- Requêtes très complexes : SQLite pas optimisé pour joins massifs ou aggregations.
- Réplication multi-agents : SQLite est single-process-writer ; pour plusieurs agents, évaluer PostgreSQL (future).
- Vérification de schéma offline : `SQLX_OFFLINE=true cargo check` exige `.sqlx/` committed (optionnel).

## Alternatives rejetées

- **ORM (Diesel, SeaORM)** : boilerplate, génération code, moins lisible que SQL écrit.
- **Fichier JSON** : impossibilité filtrer (recherche audit), slow pour large dataset.
- **redb** : pas de recherche texte (FTS5).
- **PostgreSQL** : déploiement infra (agent self-contained pas appel réseau).

## Conséquences

- Schema versioning : migrations SQL dans `crates/hearth-agent/migrations/` numérotées (20240101_init.sql, etc).
- Tests : base temporaire par test, migration appliquée ; fixtures via INSERT.
- Purge : job toutes les heures : audit > 90 j + 50 k lignes, opérations > 24 h, sessions expirées.

## Références

- SQLx docs : https://github.com/launchbadge/sqlx
- SQLite WAL : https://www.sqlite.org/wal.html
- FTS5 (full-text search) : https://www.sqlite.org/fts5.html
- Migration patterns : https://github.com/launchbadge/sqlx/tree/main/examples/todos
- ADR-0003 globale (Rust) : `orga-global/docs/adr/ADR-0003-*.md`
