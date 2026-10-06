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

La crate Tauri `hearth-desktop` est membre du workspace mais **hors `default-members`** : sous Linux elle ne compile pas (bibliothèques WebView absentes). Sous Linux, ajouter `--exclude hearth-desktop` aux commandes `--workspace` (le job Linux de la CI le fait) ; le job `desktop` de la CI (Windows) la vérifie. Sous Windows les trois commandes ci-dessus couvrent tout, **mais** `cargo clippy`/`cargo test --workspace` compilent la coquille Tauri, qui lit `apps/desktop/dist` à la compilation : sur un clone neuf, construire le front une fois d'abord (`cd apps/desktop && npm ci && npm run build`), sinon la compilation échoue. Pour tester seulement l'agent, sans le client ni le front : `cargo test --workspace --exclude hearth-desktop` (idem pour clippy).

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

## Binaire de l'agent, installation (HRT-15)

Docker est requis (conteneurs jetables ; rien n'est installé sur le poste, jamais sur un vrai serveur).

```sh
cargo xtask agent         # binaire statique x86_64-unknown-linux-musl construit dans rust:1.95-alpine ; vérifié statique (file, readelf) ; taille et SHA-256 affichés ; sortie target/dist/hearth-agent
cargo xtask e2e-install   # installation de bout en bout dans un conteneur Debian avec systemd (scénario deploy/e2e/scenario.sh) : install.sh, /hello, connexion, empreinte, réinstallation, désinstallation, purge, retour en arrière, installation gérée
cargo xtask e2e-update    # mise à jour de l'agent à distance de bout en bout (HRT-17) : clé minisign jetable, agent 0.1.0 puis 0.2.0, signature invalide, mise à jour réussie, agent muet (retour automatique), deuxième demande ; ~5 min
cargo xtask shellcheck    # deploy/install.sh et les scénarios, shellcheck en conteneur
```

Le cache Cargo du conteneur est un volume nommé (`hearth-xtask-cargo`, `hearth-xtask-target`) ; `HEARTH_XTASK_CACHE=<dossier>` le place dans ce dossier de l'hôte (c'est ce que met en cache la CI). Sous Git Bash, préfixer par `MSYS_NO_PATHCONV=1`.

`hearth-agent install [--port N] [--managed] [--yes]` et `uninstall [--keep-data|--purge] [--yes]` : interactif, ou sans question par `HEARTH_ADMIN_USER`, `HEARTH_ADMIN_PASSWORD` (ou `HEARTH_ADMIN_PASSWORD_HASH`), `HEARTH_PORT`. Droits d'administration requis. Le mot de passe n'est jamais un argument ni un exemple de ligne de commande (`hearth-agent hash-password --user NOM` fabrique le haché pour `HEARTH_ADMIN_PASSWORD_HASH`). Téléchargement : HTTPS et SHA-256 obligatoires ; x86_64 seulement. Règles `BR-INSTALL-*`, runbook `docs/runbooks/installer-agent.md`, décision `docs/adr/ADR-0012-service-systeme.md`. Mise à jour à distance (HRT-17) : `docs/runbooks/mettre-a-jour-agent.md`, ADR-0014, règles `BR-UPDATE-011` à `019`, `024` et `027` à `029` ; la clé publique de signature est embarquée (`crates/hearth-agent/update-key.pub`, sans clé secrète tant que Voiky n'a pas généré la sienne). Tout le code d'installation compile sous Windows mais ne s'exécute que sous Linux ; les tests de l'adaptateur systemd utilisent un faux `systemctl`.

## Mise à jour du client (HRT-16)

Greffon `tauri-plugin-updater` en API Rust seule (aucune permission côté web), flux `latest.json` des GitHub Releases du dépôt public, clé publique embarquée `apps/desktop/src-tauri/update-key.pub` (clé de DÉVELOPPEMENT sans clé secrète tant que Voiky n'a pas mis la sienne ; jamais de clé secrète dans le dépôt). Vérification au lancement puis 24 h au plus, état dans `update.json` côté Rust (pas de `localStorage`). Publication : `docs/runbooks/publier-une-version-du-client.md` (flux `publish-client`, déclenché à la main, brouillon). `cargo xtask client-release-check` / `client-version` / `client-sign` / `client-manifest` (la clé secrète n'est donnée qu'à `client-sign`, dans le job de signature du flux). Règles `BR-UPDATE-001` à `010`, `025`, `026` ; décision `docs/adr/ADR-0017-mise-a-jour-du-client.md`.

## Mise à jour de l'agent depuis le client (HRT-17, lot interface)

Le CLIENT lit la cible de l'agent (`agent.json`, même release que `latest.json`, ADR-0021) dans la MÊME tentative que son propre flux : une requête de plus vers le même hôte, aucune tentative de plus. La WebView ne fournit NI adresse, NI signature, NI somme : `update_agent(server_id, version)` ne reçoit que le numéro vu ; la coquille valide la cible (HTTPS public des releases du dépôt, jamais de rétrogradation) et construit la requête. L'agent reste l'arbitre (rôle, signature, somme). Le redémarrage annoncé par l'agent est une coupure ATTENDUE dans `hearth-link` (2 minutes, « Reconnexion… », sans alarme). Publication : `cargo xtask agent-manifest` puis ajout à la main à la release (runbook `docs/runbooks/mettre-a-jour-agent.md`). Règles `BR-UPDATE-020` à `023`.

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
