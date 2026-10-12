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

Les tests créent leurs dossiers temporaires sous `target/hearth-test-tmp/` (aide `crates/hearth-agent/tests/support/tmp.rs`, jamais `tempfile::tempdir()` direct), pas dans `%TEMP%`. Après une suite complète, `cargo xtask test-tmp-check` doit dire que rien n'est resté (`--clean` supprime les restes d'une suite interrompue ; la CI l'exige).

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

`hearth-agent attack-mode status|off` (HRT-25, ADR-0025) : l'état du mode attaque, et sa désactivation directement sur le serveur, sans réseau (voie de secours d'un administrateur qui ne peut pas prouver de clé depuis le client). Il n'existe pas de sous-commande pour l'activer.

## Binaire de l'agent, installation (HRT-15)

Docker est requis (conteneurs jetables ; rien n'est installé sur le poste, jamais sur un vrai serveur).

```sh
cargo xtask agent         # binaire statique x86_64-unknown-linux-musl construit dans rust:1.95-alpine ; vérifié statique (file, readelf) ; taille et SHA-256 affichés ; sortie target/dist/hearth-agent
cargo xtask e2e-install   # installation de bout en bout dans un conteneur Debian avec systemd (scénario deploy/e2e/scenario.sh) : install.sh, /hello, connexion, empreinte, réinstallation, désinstallation, purge, retour en arrière, installation gérée
cargo xtask e2e-double-failure  # double panne de la mise à jour (HRT-27) sur le systemd jetable, avec le binaire de `agent` ; `deploy/e2e/double-failure-systemd.sh`
cargo xtask e2e-update    # mise à jour de l'agent à distance de bout en bout (HRT-17) : clé minisign jetable, agent 0.1.0 puis 0.2.0, signature invalide, mise à jour réussie, agent muet (retour automatique), deuxième demande, superviseur tué après l'échange (HRT-27 : relancé par systemd, retour automatique sans geste) ; ~5 min. Double panne seule, sur un systemd quelconque (root, installe puis désinstalle l'agent) : `sudo sh deploy/e2e/double-failure-systemd.sh <hearth-agent>`
cargo xtask shellcheck    # deploy/install.sh et les scénarios, shellcheck en conteneur
```

Le cache Cargo du conteneur est un volume nommé (`hearth-xtask-cargo`, `hearth-xtask-target`) ; `HEARTH_XTASK_CACHE=<dossier>` le place dans ce dossier de l'hôte (c'est ce que met en cache la CI). Sous Git Bash, préfixer par `MSYS_NO_PATHCONV=1`.

`hearth-agent install [--port N] [--managed] [--yes]` et `uninstall [--keep-data|--purge] [--yes]` : interactif, ou sans question par `HEARTH_ADMIN_USER`, `HEARTH_ADMIN_PASSWORD` (ou `HEARTH_ADMIN_PASSWORD_HASH`), `HEARTH_PORT`. Droits d'administration requis. Le mot de passe n'est jamais un argument ni un exemple de ligne de commande (`hearth-agent hash-password --user NOM` fabrique le haché pour `HEARTH_ADMIN_PASSWORD_HASH`). Téléchargement : HTTPS et SHA-256 obligatoires ; x86_64 seulement. Règles `BR-INSTALL-*`, runbook `docs/runbooks/installer-agent.md`, décision `docs/adr/ADR-0012-service-systeme.md`. Mise à jour à distance (HRT-17) : `docs/runbooks/mettre-a-jour-agent.md`, ADR-0014, règles `BR-UPDATE-011` à `019`, `024` et `027` à `035` ; la clé publique de signature est embarquée (`crates/hearth-agent/update-key.pub`, identifiant de clé `29944AFE28EABECE`, dans le dépôt depuis le 2026-10-12 ; la clé secrète n'est jamais dans le dépôt et ne se régénère qu'en cas de perte ou de rotation, qui laisse les agents installés sans mise à jour à distance). Tout le code d'installation compile sous Windows mais ne s'exécute que sous Linux ; les tests de l'adaptateur systemd utilisent un faux `systemctl`.

## Mise à jour du client (HRT-16)

Greffon `tauri-plugin-updater` en API Rust seule (aucune permission côté web), flux `https://hearth.voikyrioh.fr/api/releases/latest` (serveur Hearth, ADR-0035 ; plus les GitHub Releases), clé publique embarquée `apps/desktop/src-tauri/update-key.pub` (clé publique de publication ; la clé secrète n'est jamais dans le dépôt). Vérification au lancement puis 24 h au plus, état dans `update.json` côté Rust (pas de `localStorage`). Publication : `docs/runbooks/publier-une-version-du-client.md` (flux `publish-client`, déclenché à la main : il laisse un artefact signé, ni brouillon ni release GitHub ; le dépôt se fait ensuite par `cargo xtask release-stage` puis `release-upload`, voir plus bas). `cargo xtask client-release-check` / `client-version` / `client-sign` / `client-manifest` (la clé secrète n'est donnée qu'à `client-sign`, dans le job de signature du flux). Règles `BR-UPDATE-001` à `010`, `025`, `026` ; décisions `docs/adr/ADR-0017-mise-a-jour-du-client.md` et `docs/adr/ADR-0035-les-mises-a-jour-sont-servies-par-hearth-voikyrioh-fr.md` (adresses `https://hearth.voikyrioh.fr/api/releases/{X.Y.Z}/{fichier}`, sans `v` ; `client-release-check` refuse de resigner un numéro déjà étiqueté `vX.Y.Z` sur `origin` ; l'étiquette est posée à la fin, à la main ; `release-upload` ferme la fenêtre entre signature et étiquette à partir du premier objet déposé (BR-RELEASE-001) : un numéro signé puis jamais déposé reste resignable).

## Mise à jour de l'agent depuis le client (HRT-17, lot interface)

Le CLIENT lit la cible de l'agent dans la section `agent` du manifeste de versions (ADR-0021, ADR-0035), dans la MÊME requête que son propre flux (`version_comparator` du greffon) : toujours UNE requête par vérification (BR-UPDATE-001). La WebView ne fournit NI adresse, NI signature, NI somme : `update_agent(server_id, version)` ne reçoit que le numéro vu ; la coquille valide la cible (HTTPS public, `hearth.voikyrioh.fr/api/releases/`, jamais de rétrogradation) et construit la requête. L'agent reste l'arbitre (rôle, signature, somme). Le redémarrage annoncé par l'agent est une coupure ATTENDUE dans `hearth-link` (2 minutes, « Reconnexion… », sans alarme). Publication : l'agent se construit (`cargo xtask agent`) et se signe à la main avec `minisign` (commentaire signé `version:X.Y.Z`), puis `cargo xtask release-stage` écrit lui-même la section `agent` du manifeste (adresse `https://hearth.voikyrioh.fr/api/releases/{version de l'agent}/hearth-agent-linux-x86_64`) et `release-upload` dépose (voir « Dépôt d'une version (HRT-51) » ; `agent-manifest` n'est plus dans la procédure ; runbook `docs/runbooks/mettre-a-jour-agent.md`). Règles `BR-UPDATE-020` à `023`.

### Dépôt d'une version (HRT-51)

```sh
cargo xtask release-stage --client-dir <artefact publish-client> --agent-binary <hearth-agent> --agent-signature <hearth-agent.minisig> [--install-script <autre fichier>]
cargo xtask release-upload             # ESSAI À BLANC (défaut) : affiche chaque écriture, ne lance aucune commande
cargo xtask release-upload --confirm   # dépose : fichiers de la version, relecture publique, script, manifeste EN DERNIER (chacun relu dans le bucket)
```

`release-stage` écrit sous `target/release-stage/` seulement, après avoir revérifié signatures (client et agent avec version signée), sommes, adresses, liste fermée de l'API, forme du manifeste et script (non vide, `#!/bin/sh`, lit `releases/latest/agent`) ; la version est celle du dépôt, l'agent est rangé dans le dossier de SA version. Sans `--install-script`, `deploy/install.sh` doit être identique à `HEAD` et ce sont les octets de `HEAD` (`git show`) qui sont déposés, pas ceux du disque (le commit est affiché) ; si l'option désigne ce même fichier du dépôt, la même garde s'applique. `--install-script` ne sert que pour un AUTRE fichier (lu sur le disque, non comparé à `HEAD`). `release-upload --confirm` exige l'outil `aws` (AWS CLI v2) et `curl`, et les variables `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` de la clé de publication dans l'environnement (lues par `aws`, jamais par `xtask`, jamais en argument) ; il refuse d'écrire une clé de version déjà présente avec un autre contenu, et de remplacer un manifeste en place de version supérieure (sauf `--allow-rollback`, qui laisse une ligne « RETOUR ARRIÈRE » avant la première écriture). Chaque objet part des octets vérifiés en mémoire (`aws s3 cp -`). Droits requis de la clé de publication : LECTURE d'objet (`HeadObject`, `GetObject`) et ÉCRITURE (`PutObject`, dépôts en plusieurs parties compris), SANS suppression ; sans droit de lister, une clé absente peut répondre 403 au lieu de 404 : l'outil s'arrête alors (« état impossible à établir »), à vérifier à la console avant le premier dépôt. Règles `BR-RELEASE-001` à `003`, code `xtask/src/release_stage.rs` et `release_publish.rs`.

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
