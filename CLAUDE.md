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

## Moteur de conteneurs (HRT-61)

L'agent lit le moteur de conteneurs (Docker) par son socket Unix et diffuse la liste des services, en LECTURE SEULE (ADR-0036 à ADR-0038). Réglages : `[engine] enabled` et `[engine] socket` de `agent.toml`, `HEARTH_ENGINE=off` (désactive : le socket n'est jamais ouvert), `HEARTH_ENGINE_SOCKET`. Aucun moteur sur le poste de développement : tout se teste contre le faux moteur (`FakeEngine`) et un faux démon HTTP local ; le vrai moteur n'existe qu'en CI (job `engine`, `HEARTH_TEST_ENGINE=1 cargo test -p hearth-agent --test engine_real -- --ignored`). Tests bridés : `cargo test -p hearth-agent -j 2 --test services_https -- --test-threads=2`. **Publication (HRT-79, ADR-0043, `BR-ADOPT-*`)** : l'agent lit TOUS les conteneurs mais ne publie que les services créés par Hearth (`h_`) et les services adoptés (table `adopted_services`, migration `0011`, écrite par `ServiceAdoption`, HRT-80 : `POST /services/adopt` et `POST /services/{key}/release`, `GET /services/external` pour les candidats ; réglage `[services] adoption`, `HEARTH_SERVICES_ADOPTION=off` ; aide de test `Env::adopt`) ; les autres sont comptés (`engine.external_count`). Le filtre est `ServicesRegistry::publish` (`domain/services/visibility.rs::visibility_of`), appelé à un seul endroit : ne jamais lire `inner.containers` pour répondre à un client ni logguer un nom/une image/un port/une unité d'un conteneur (test sentinelle `tests/services_https.rs`). Aucune valeur de variable dans les types du fil sauf la réponse de `GET /services/{key}/env` (comptes qui ont le droit « gérer les services », indépendant du rôle et absent par défaut, `Access::Services`, ADR-0041, migration `0010` ; `Concealed`, consignée) ; les noms de variables et la commande (détail d'un service, HRT-63) sont dans `Hidden`/`Concealed` et ne vont dans aucune trace. Services lancés par une unité systemd (clé `u_`) : `ServiceSupervisor` (détection seule, `/proc` + `systemctl show`), migration `0009` ; l'étape CI `supervised` (`tests/engine_supervised.rs`, root) consigne ce que fait un arrêt par l'API du moteur sur un conteneur d'unité.

Journaux d'un service (HRT-66, ADR-0038) : `GET /services/{key}/logs` est un WebSocket par lecture (jamais `/stream`), 1 000 dernières lignes puis le direct, anneau borné par lecture, dix lectures au plus, reprise par curseur (heure en ms et rang), suivi de la CLÉ du service d'un conteneur à l'autre. Le texte d'une ligne est du contenu sensible : aucune trace, aucun `Debug`, jamais stocké (test sentinelle `tests/service_logs_ws.rs`). Nettoyage et coupe : `hearth_proto::log_text` seulement. Lecture du moteur écrite à la main (`infrastructure/engine/logs.rs`), pas `bollard::logs`. « Ce compte lit ce service » : `domain::services::logs::may_read(viewer, target)`, un seul point (reçoit le compte et l'origine du service, rejoué pendant la lecture ; HRT-76 et HRT-79 y ajouteront leurs règles). Règles `BR-SLOG-*`. Tests bridés : `cargo test -p hearth-agent -j 2 --test service_logs_ws -- --test-threads=2`.

## Comptes en ligne de commande

`hearth-agent account add|list|passwd|role|remove|revoke` (voir `docs/runbooks/recuperer-acces-administrateur.md`). Le mot de passe n'est jamais un argument : saisie sans écho, ou `HEARTH_ACCOUNT_PASSWORD`.

`hearth-agent account services {compte} --grant|--revoke` (HRT-77, BR-GRANT-008, ADR-0041) : donne ou retire le droit « gérer les services », **en root seulement** (contrôlé avant d'ouvrir la base), consigné au journal (origine « ligne de commande du serveur »), sans effet ni entrée si l'état est déjà atteint ; c'est avec la question de l'installation le seul chemin qui écrit `accounts.services_right` (`infrastructure/sqlite/services_right_repo.rs`, garde `tests/services_right_never_written.rs`). Runbook `docs/runbooks/accorder-le-droit-de-gerer-les-services.md`.

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

`hearth-agent install [--port N] [--managed] [--yes] [--services|--no-services]` (HEARTH_SERVICES=manage|none ; question « Ce compte pourra-t-il déployer et piloter des services sur ce serveur ? » si un moteur est détecté, sans terminal ou `--yes` : non, BR-GRANT-006, 007) et `uninstall [--keep-data|--purge] [--yes]` : interactif, ou sans question par `HEARTH_ADMIN_USER`, `HEARTH_ADMIN_PASSWORD` (ou `HEARTH_ADMIN_PASSWORD_HASH`), `HEARTH_PORT`. Droits d'administration requis. Le mot de passe n'est jamais un argument ni un exemple de ligne de commande (`hearth-agent hash-password --user NOM` fabrique le haché pour `HEARTH_ADMIN_PASSWORD_HASH`). Téléchargement : HTTPS et SHA-256 obligatoires ; x86_64 seulement. Règles `BR-INSTALL-*`, runbook `docs/runbooks/installer-agent.md`, décision `docs/adr/ADR-0012-service-systeme.md`. Mise à jour à distance (HRT-17) : `docs/runbooks/mettre-a-jour-agent.md`, ADR-0014, règles `BR-UPDATE-011` à `019`, `024` et `027` à `035` ; la clé publique de signature est embarquée (`crates/hearth-agent/update-key.pub`, identifiant de clé `29944AFE28EABECE`, dans le dépôt depuis le 2026-10-12 ; la clé secrète n'est jamais dans le dépôt et ne se régénère qu'en cas de perte ou de rotation, qui laisse les agents installés sans mise à jour à distance). Tout le code d'installation compile sous Windows mais ne s'exécute que sous Linux ; les tests de l'adaptateur systemd utilisent un faux `systemctl`.

## Mise à jour du client (HRT-16)

Greffon `tauri-plugin-updater` en API Rust seule (aucune permission côté web), flux `https://hearth.voikyrioh.fr/api/releases/latest` (serveur Hearth, ADR-0035 ; plus les GitHub Releases), clé publique embarquée `apps/desktop/src-tauri/update-key.pub` (clé publique de publication ; la clé secrète n'est jamais dans le dépôt). Vérification au lancement puis 24 h au plus, état dans `update.json` côté Rust (pas de `localStorage`). Publication : `docs/runbooks/publier-une-version-du-client.md` (flux `publish-client`, déclenché à la main : il laisse un artefact signé, ni brouillon ni release GitHub ; le dépôt se fait ensuite par `cargo xtask release-stage` puis `release-upload`, voir plus bas). `cargo xtask client-release-check` / `client-version` / `client-sign` / `client-manifest` (la clé secrète n'est donnée qu'à `client-sign`, dans le job de signature du flux). Règles `BR-UPDATE-001` à `010`, `025`, `026` ; décisions `docs/adr/ADR-0017-mise-a-jour-du-client.md` et `docs/adr/ADR-0035-les-mises-a-jour-sont-servies-par-hearth-voikyrioh-fr.md` (adresses `https://hearth.voikyrioh.fr/api/releases/{X.Y.Z}/{fichier}`, sans `v` ; `client-release-check` refuse de resigner un numéro déjà étiqueté `vX.Y.Z` sur `origin` ; l'étiquette est posée à la fin, à la main ; `release-upload` ferme la fenêtre entre signature et étiquette à partir du premier objet déposé (BR-RELEASE-001) : un numéro signé puis jamais déposé reste resignable).

## Mise à jour de l'agent depuis le client (HRT-17, lot interface)

Le CLIENT lit la cible de l'agent dans la section `agent` du manifeste de versions (ADR-0021, ADR-0035), dans la MÊME requête que son propre flux (`version_comparator` du greffon) : toujours UNE requête par vérification (BR-UPDATE-001). La WebView ne fournit NI adresse, NI signature, NI somme : `update_agent(server_id, version)` ne reçoit que le numéro vu ; la coquille valide la cible (HTTPS public, `hearth.voikyrioh.fr/api/releases/`, jamais de rétrogradation) et construit la requête. L'agent reste l'arbitre (rôle, signature, somme). Le redémarrage annoncé par l'agent est une coupure ATTENDUE dans `hearth-link` (2 minutes, « Reconnexion… », sans alarme). Publication : l'agent se construit (`cargo xtask agent`) et se signe à la main avec `minisign` (commentaire signé `version:X.Y.Z`), puis `cargo xtask release-stage` écrit lui-même la section `agent` du manifeste (adresse `https://hearth.voikyrioh.fr/api/releases/{version de l'agent}/hearth-agent-linux-x86_64`) et `release-upload` dépose (voir « Dépôt d'une version (HRT-51) » ; `agent-manifest` n'est plus dans la procédure ; runbook `docs/runbooks/mettre-a-jour-agent.md`). Règles `BR-UPDATE-020` à `023`.

### Dépôt d'une version (HRT-51)

```sh
cargo xtask release-stage --client-dir <artefact publish-client> --agent-binary <hearth-agent> --agent-signature <hearth-agent.minisig> [--install-script <autre fichier>]
cargo xtask release-upload             # ESSAI À BLANC (défaut) : affiche chaque écriture, ne lance ni aws ni curl (seul git, en lecture)
cargo xtask release-upload --confirm   # dépose : fichiers de la version, relecture publique, script, manifeste EN DERNIER (chacun relu dans le bucket)
```

`release-stage` refuse AVANT toute écriture un arbre qui n'est pas son commit (HRT-60, BR-RELEASE-003) : fichier SUIVI modifié (indexé ou non, `git status --porcelain --untracked-files=no`), fichier NON SUIVI et non ignoré sous les entrées de la construction de l'agent (`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.cargo`, `.sqlx`, `crates` : une migration ou un `build.rs` non suivi entre dans le binaire), fichier IGNORÉ sous ces mêmes chemins (l'ignorance peut venir de `.git/info/exclude` ou du fichier d'exclusion global, locaux au poste), fichier masqué à git (`skip-worktree`, `assume-unchanged`, vus par `git ls-files -v`). Les autres fichiers non suivis (`target/`, artefact du client, signatures) sont tolérés ; un `git` muet n'est jamais lu comme « propre ». Il nomme les fichiers (dix au plus), lit `HEAD` (relu une fois pour le script et comparé, le script étant lu par `git show <ce commit>`), l'affiche et l'écrit dans `target/release-stage/COMMIT`, à côté de `releases/` (jamais déposé, hors de la liste fermée). `release-upload`, essai à blanc compris, refuse si ce fichier manque ou si le commit ou l'arbre en place ont changé (seul git, en lecture, tourne avant le premier `aws` ou `curl`). Le binaire donné par `--agent-binary` doit avoir été construit sur ce commit : `cargo xtask agent` écrit, à côté de `target/dist/hearth-agent`, `hearth-agent.build` (quatre lignes `commit=`, `tree=clean|dirty|unknown`, `sha256=`, `version=`, `clean` seulement si l'arbre est propre avant ET après la construction, sur le même commit) et `release-stage` refuse un binaire sans enregistrement, de somme différente, d'un autre commit que `HEAD` ou construit sur un arbre pas propre (aucune option de contournement ; l'enregistrement n'est jamais assemblé ni déposé). `cargo xtask agent` affiche le commit construit et avertit si l'arbre n'est pas le commit, sans refuser (la commande sert au développement et aux tâches e2e ; un git qui échoue n'y fait pas échouer la construction). Limites : l'enregistrement est un fichier en clair, falsifiable à la main (garde contre l'erreur, pas contre la malveillance) ; le contrôle de l'arbre est fait à l'assemblage et au dépôt, avec de petites fenêtres entre ses lectures et celles de `Cargo.toml` et des clés ; l'installateur du client vient de l'artefact du flux CI, rien ne le relie à ce commit ; l'image du conteneur est figée par empreinte, mais les paquets `apk` et les volumes de cache de Cargo ne sont pas vus par cette garde ; rien ne vérifie que `HEAD` est le commit qui sera étiqueté. `release-stage` écrit sous `target/release-stage/` seulement, après avoir revérifié signatures (client et agent avec version signée), sommes, adresses, liste fermée de l'API, forme du manifeste et script (non vide, `#!/bin/sh`, lit `releases/latest/agent`) ; la version est celle du dépôt, l'agent est rangé dans le dossier de SA version. Sans `--install-script`, `deploy/install.sh` doit être identique à `HEAD` et ce sont les octets de `HEAD` (`git show`) qui sont déposés, pas ceux du disque (le commit est affiché) ; si l'option désigne ce même fichier du dépôt, la même garde s'applique. `--install-script` ne sert que pour un AUTRE fichier (lu sur le disque, non comparé à `HEAD`). `release-upload --confirm` exige l'outil `aws` (AWS CLI v2) et `curl`, et les variables `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` de la clé de publication dans l'environnement (lues par `aws`, jamais par `xtask`, jamais en argument) ; il refuse d'écrire une clé de version déjà présente avec un autre contenu, et de remplacer un manifeste en place de version supérieure (sauf `--allow-rollback`, qui laisse une ligne « RETOUR ARRIÈRE » avant la première écriture). Chaque objet part des octets vérifiés en mémoire (`aws s3 cp -`). Droits requis de la clé de publication : LECTURE d'objet (`HeadObject`, `GetObject`) et ÉCRITURE (`PutObject`, dépôts en plusieurs parties compris), SANS suppression ; sans droit de lister, une clé absente peut répondre 403 au lieu de 404 : l'outil s'arrête alors (« état impossible à établir »), à vérifier à la console avant le premier dépôt. Règles `BR-RELEASE-001` à `003`, code `xtask/src/release_stage.rs` et `release_publish.rs`.

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
