---
id: ADR-0036
titre: Accès au moteur de conteneurs par l'API de son socket Unix (bollard, sans TLS), derrière le port ContainerEngine
type: librairie
statut: proposée (non validée par Voiky ; posée avec HRT-61)
date: 2026-10-13
portee: repo
remplace: aucune
liens: [ADR-0003, ADR-0009, ADR-0012, conception technique 2026-10-13 (déploiements, lot 1) sections 3, 9 et 10, BR-ENGINE-002, BR-ENGINE-011]
---

# ADR-0036 : Accès au moteur de conteneurs

## Contexte
Le lot 1 de l'épic déploiements fait lister, piloter, lire et créer des conteneurs par l'agent. L'agent est un binaire statique musl (ADR-0003), sans `aws-lc` ni OpenSSL (ADR-0009), qui tourne en root (ADR-0012). Le poste de développement n'a pas de moteur en marche : l'accès doit se tester sans lui.

## Décision
1. L'agent parle au moteur par l'**API HTTP de son socket Unix** (`/var/run/docker.sock`, réglable par `engine.socket` et `HEARTH_ENGINE_SOCKET`). `DOCKER_HOST` n'est pas lu ; un moteur distant (TCP) n'est pas pris en charge.
2. Crate **`bollard`**, sans TLS : `bollard = { version = "0.21", default-features = false, features = ["http", "pipe"] }`. Les fonctionnalités `ssl`, `aws-lc-rs`, `buildkit`, `ssh` restent éteintes.
3. Tout passe par le port `application/ports/container_engine.rs` (`ContainerEngine`). Aucun type de `bollard` ne sort de `infrastructure/engine/`. Les erreurs du moteur sont traduites en `EngineError` typé dans `infrastructure/engine/errors.rs` ; un texte inconnu devient `Refused`, jamais affiché brut.
4. **Version minimale : Docker Engine 20.10 (API 1.41)**, version d'API négociée avec le démon.
5. **Seul Docker est pris en charge.** Un moteur qui s'annonce autrement (Podman) donne l'état « non pris en charge », même s'il parle l'API compatible.
6. Réglage `engine.enabled` (`[engine] enabled = false` dans `agent.toml`, ou `HEARTH_ENGINE=off`) : à `false`, l'agent n'ouvre jamais le socket et annonce l'état « désactivé ». Le socket se règle par `[engine] socket` ou `HEARTH_ENGINE_SOCKET`. L'option d'installation `--no-engine` n'existe pas encore (hors tranche 1).

## Version
`bollard` 0.21.1 (publiée le 2026-08-16).

## Installation
Ligne ci-dessus dans `[workspace.dependencies]`, `bollard.workspace = true` dans `crates/hearth-agent/Cargo.toml`.

## Usage
```rust
// infrastructure/engine/bollard.rs : la seule construction du client, à la version d'API du moteur
// (lue dans l'en-tête `Api-Version` de `GET /_ping`), plafonnée à celle de bollard.
let docker = Docker::connect_with_unix(&socket, 10, &ClientVersion { major_version: 1, minor_version: 43 })?;
```
En test, le même adaptateur reçoit `Target::Http` (client `Docker::connect_with_http`) branché sur un faux démon local.

## Mise en oeuvre (HRT-61, tranche 1 : lecture seule)
- **Version et fonctionnalités vérifiées** : `bollard 0.21.1`, `default-features = false`, `features = ["http", "pipe"]` (déclarées une fois dans `[workspace.dependencies]`). `cargo tree -p hearth-agent -i openssl-sys`, `-i aws-lc-rs` et `-i native-tls` répondent « did not match any packages » : aucune pile TLS native n'entre. Paquets ajoutés à `Cargo.lock` : quatre seulement, `bollard`, `bollard-stubs`, `hyperlocal` (socket Unix) et `hyper-named-pipe` (Windows seulement) ; tout le reste (`hyper`, `hyper-util`, `http`, `tokio-util`, `serde`…) était déjà dans l'arbre. `cargo tree -p hearth-agent --target x86_64-unknown-linux-musl` ne trouve non plus aucune de ces piles ni crate à code C dans le sous-arbre de `bollard` (seulement `libc`). Pas de `deny.toml` ni de `cargo vet` dans ce dépôt. **Non vérifié sur le poste de développement** : le lien statique musl (`cargo xtask agent` demande Docker) ; c'est le job `agent-install` de la CI qui le prouve. Le repli (client sur hyper) reste décrit plus bas.
- **La sonde ne passe pas par bollard.** bollard préfixe TOUT chemin d'une version d'API, `/_ping` compris, et son défaut (1.53) est refusé par un Docker 20.10 (`client version 1.53 is too new`). L'adaptateur ouvre donc le socket lui-même, envoie `GET /_ping` sans version et lit `Api-Version`, ce qui distingue aussi socket absent (`ENOENT`), connexion refusée, `EACCES` (ou 401/403 d'un proxy de socket) et silence de plus de 5 secondes ; puis il construit le client bollard à la version annoncée (plafonnée) pour `/version`, la liste, l'inspection et les événements.
- **Le nom du moteur** est celui de `/version` : un composant « Podman… » s'il y en a un, sinon `Platform.Name`, sinon « Docker Engine » (Debian laisse `Platform.Name` vide). Seuls les noms qui contiennent « docker » ou « moby » sont pris en charge.
- **Installé ou non** (séparer « arrêté » d'« absent ») : présence de `dockerd` ou d'une unité `docker.service` / `docker.socket` aux emplacements courants (Debian, Fedora, Arch, NixOS), en lisant des métadonnées seulement.
- **Ce qui ne sort pas de l'adaptateur** : `Config.Env`, `Config.Cmd`, `Entrypoint`, `Path`, `Args` d'une inspection ne sont jamais copiés dans une `Observation`.
- **Port en lecture seule** : `probe`, `list`, `inspect`, `events`. Les méthodes d'écriture arrivent avec les tranches qui en ont besoin, chacune avec son faux et son cas dans la suite de contrat.
- **Tests** : une suite de contrat (`tests/support/engine_contract.rs`) contre `FakeEngine`, contre un faux démon HTTP local dont les réponses sont FABRIQUÉES d'après la documentation de l'API (pas enregistrées d'un vrai moteur) et, en CI seulement, contre un vrai moteur (job `engine`).

## Doc
- https://docs.rs/bollard
- https://github.com/fussybeaver/bollard
- https://docs.docker.com/reference/api/engine/

## Périmètre
- Utilisée dans `crates/hearth-agent/src/infrastructure/engine/` seulement.
- Jamais dans `domain/`, `application/`, `entrypoint/`, `hearth-proto`, `hearth-link`, la coquille.

## Comment l'appliquer
- Un besoin nouveau envers le moteur (ressources, statistiques, console) = une méthode de plus sur `ContainerEngine`, son faux (`infrastructure/engine/fake.rs`) et son cas dans la suite de contrat (`tests/support/engine_contract.rs`), exécutée contre le faux, le faux démon et un vrai moteur en CI.
- Vérification à chaque montée de version : `cargo tree -p hearth-agent -i aws-lc-rs` et `-i openssl` ne trouvent rien ; `cargo xtask agent` rend un binaire statique.

## Quand NE PAS l'appliquer / limites
- **Le socket du moteur équivaut à root** : qui le pilote monte `/` dans un conteneur. L'agent étant root, il n'obtient aucun droit de plus ; mais tout compte Administrateur de Hearth devient, par l'agent, root du serveur. Les protections sont celles du socle (TLS épinglé, acte confirmé par clé de poste et mot de passe, journal). À dire dans `SECURITY.md`.
- Installation gérée où l'agent n'est pas root : l'état « Accès refusé à l'agent » nomme le compte ; l'ajouter au groupe du moteur lui donne root.
- **Docker en paquet Snap** : le socket est `/var/snap/docker/common/var-run/docker.sock`, pas `/var/run/docker.sock`, et l'agent confiné ou non peut ne pas y avoir droit. Non géré dans cette tranche : le chemin se règle à la main (`[engine] socket`), et les repères d'installation ne contiennent ni le binaire ni l'unité du paquet Snap : un moteur Snap arrêté apparaît « absent » au lieu d'« arrêté ».
- **OpenRC (Alpine, Gentoo) et autres inits sans systemd** : la détection par l'API du socket marche, mais l'agent ne distingue que « systemd » d'« autre » (`host.init`, présence de `/run/systemd/system`) et ses repères d'installation ne regardent que des unités systemd : sur OpenRC, un moteur installé mais arrêté peut apparaître « absent » si son binaire n'est pas à un emplacement connu, et aucune commande de démarrage propre à OpenRC (`rc-service docker start`) n'est produite. À traiter avec la tranche des actions.
- Si `bollard` ne se lie pas en statique ou tire une dépendance interdite : écrire un client minimal sur hyper derrière le même port.

## Alternatives rejetées
- **Ligne de commande `docker` en sous-processus** : sortie texte variable, progression de téléchargement illisible sans terminal, erreurs non typées, un processus par flux, binaire à trouver.
- **Client maison sur hyper** : routes, démultiplexage des journaux et modèles à écrire et à suivre ; gardé comme repli.
- **Utilisateur dédié dans le groupe `docker`** : déjà écarté par l'ADR-0012 (équivalent à root).

## Conséquences
- Une dépendance de plus dans l'agent et ses transitives (`hyperlocal`, modèles générés).
- Un job CI `engine` avec un vrai moteur.
- Podman reste à une garde et un job CI de la prise en charge.

## Références
- crates.io, `bollard` (versions et fonctionnalités lues le 2026-10-13).
- ADR-0003, ADR-0009, ADR-0012.
