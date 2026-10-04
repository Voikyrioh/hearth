# Hearth — guide pour Claude

Hub et pipeline : `CLAUDE.md` du hub `orga-global` (dépôt privé, prime sur tout). Contexte produit : `contexts/hearth/` du hub (épics, stories, specs, conception technique, tickets `HRT-*`, sessions).

## Avant de coder

1. `ARCHITECTURE.md` puis `docs/INDEX.md`.
2. `docs/adr/` (décisions) et `docs/business-rules/` (règles du domaine touché).
3. Conception : `contexts/hearth/conceptions/2026-10-04-technique-socle.md` du hub (privé) ; les décisions reprises ici sont dans `docs/adr/`.

## Commandes

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Les trois doivent passer avant tout commit.

### Client Windows (`apps/desktop`)

La crate Tauri `hearth-desktop` est membre du workspace mais **hors `default-members`** : sous Linux elle ne compile pas (bibliothèques WebView absentes). Sous Linux, ajouter `--exclude hearth-desktop` aux commandes `--workspace` (le job Linux de la CI le fait) ; le job `desktop` de la CI (Windows) la vérifie. Sous Windows les trois commandes ci-dessus couvrent tout.

```sh
cd apps/desktop
npm ci
npm run lint && npm run typecheck && npm test     # Biome, vue-tsc, Vitest
npm run build                                     # requis avant clippy/test Rust de la coquille (dist/ lu à la compilation)
npm run tauri dev                                 # application complète ; régénère src/bindings.ts
npm run tauri build                               # installateur NSIS dans target/release/bundle/nsis/
```

`src/bindings.ts` est généré (tauri-specta) et versionné ; s'il est périmé, `cargo test -p hearth-desktop` échoue : `HEARTH_REGEN_BINDINGS=1 cargo test -p hearth-desktop`. Détails et ajout d'une commande : `docs/adr/ADR-0010-dependances-client.md`.

## SQLx (requêtes vérifiées, mode hors ligne)

Les `query!`/`query_as!` sont vérifiées à la compilation contre `.sqlx/` (versionné). Sans `DATABASE_URL`, `cargo build` et la CI lisent ce dossier : rien à configurer. Après toute modification d'une requête `query!` ou d'une migration, régénérer et committer `.sqlx/` :

```sh
export DATABASE_URL="sqlite://$PWD/target/sqlx-dev.db"     # base de dev jetable ; sous Windows, chemin du type sqlite://J:/.../target/sqlx-dev.db
cargo sqlx database create && cargo sqlx migrate run --source crates/hearth-agent/migrations
cargo sqlx prepare --workspace -- --all-targets
git add .sqlx
```

Si `cargo sqlx` manque ou n'a pas le pilote SQLite : `cargo install sqlx-cli --no-default-features --features sqlite,rustls`. Voie sans le CLI : créer la base en exécutant le SQL des migrations (par exemple avec le module `sqlite3` de Python), puis `DATABASE_URL=… SQLX_OFFLINE_DIR=$PWD/.sqlx cargo build -p hearth-agent --all-targets` après avoir touché `crates/hearth-agent/src/lib.rs` pour forcer la recompilation. Les migrations déjà publiées ne se modifient jamais : on en ajoute une.

## Comptes en ligne de commande

`hearth-agent account add|list|passwd|role|remove|revoke` (voir `docs/runbooks/recuperer-acces-administrateur.md`). Le mot de passe n'est jamais un argument : saisie sans écho, ou `HEARTH_ACCOUNT_PASSWORD`.

## Règles

- Architecture hexagonale dans `hearth-agent` et `hearth-link` : `domain/` sans E/S ni dépendance vers axum, SQLx, système. Ports dans `application/ports/`.
- Une règle métier vit dans `domain/` et a sa fiche `docs/business-rules/BR-*.md`, modifiée dans le même commit.
- `unwrap` et `expect` interdits hors tests (lint en erreur). Erreurs : `thiserror` par couche, pas d'`anyhow` ; l'erreur fatale est journalisée une seule fois dans `main`.
- Un port est consommé par la racine de composition (`app.rs`) via `Arc<dyn Port>` ou `&dyn Port`, jamais par son type concret. `application` ne connaît ni axum ni SQLx ; `entrypoint` n'importe pas `infrastructure`. Mots de passe et hachés : type `Secret`, jamais dans un message ou un journal.
- `hearth-proto` : aucun type de framework, aucune E/S.
- Le client ne parle à l'agent que par `hearth-link`. L'interface web n'ouvre aucune connexion réseau.
- Interface (`apps/desktop/src`) : Composition API et `<script setup lang="ts">`, Atomic design, zéro `style=` en ligne, SVG en composants atomes, jetons CSS uniquement (`styles/tokens.css`), textes dans `i18n/fr.ts`.
- Textes d'interface : français, tutoiement, pas de tiret cadratin.
- Git : une branche par ticket (`feat/HRT-nn-slug`), PR, pas de push direct sur `main`. Gel des commits en heures ouvrées (voir hub).
