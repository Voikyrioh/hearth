---
id: ADR-0035
titre: Les mises à jour sont servies par hearth.voikyrioh.fr (flux, installateur et cible de l'agent) et non plus par les GitHub Releases
type: securite
statut: acceptée
date: 2026-10-12
portee: projet
remplace: partie « flux GitHub Releases » d'ADR-0017 (décision 2) et d'ADR-0021 (décision 4, source permise)
liens: [ADR-0008, ADR-0014, ADR-0017, ADR-0021, conception technique 2026-10-09 distribution Scaleway (hub, sections 1, 4.4, 8, 10.2), HRT-49]
---

# ADR-0035 : Les mises à jour sont servies par hearth.voikyrioh.fr

## Contexte

ADR-0017 a fixé le flux de versions aux GitHub Releases du dépôt public ; ADR-0021 y a rangé la cible de l'agent. Aucune version n'a encore été publiée. Voiky a décidé (2026-10-09, validé le 2026-10-12) de servir les binaires et le manifeste depuis son infrastructure, sous un seul nom, `hearth.voikyrioh.fr`, tout ce qui est API sous `/api`. Le code source reste sur GitHub.

L'adresse du flux est une constante du client (comme la clé publique) : elle doit être posée avant la 0.1.0 et ne plus changer. Cet ADR couvre la partie CLIENT et OUTILLAGE de publication (HRT-49). Le service lui-même (`hearth-releases-api`), le dépôt dans le bucket, le flux `publish-client` et `install.sh` sont d'autres tickets.

## Décision

1. **Adresses compilées, définitives** (`apps/desktop/src-tauri/src/update/domain.rs`) : `FEED_URL = https://hearth.voikyrioh.fr/api/releases/latest` (route d'API, sans extension, servie en `application/json` ; le greffon `tauri-plugin-updater` 2.13.1 lit le corps sans regarder ni le nom ni le type, et le serveur ne répond jamais 204), `DOWNLOAD_HOST = hearth.voikyrioh.fr`, `DOWNLOAD_PATH_PREFIX = /api/releases/`. `MAX_REDIRECTS` reste 5 : aucune redirection dans le parcours nominal, la borne est l'issue de secours pour déplacer le service sans toucher aux clients.
2. **Une seule politique de source** : `DownloadPolicy::official_releases()` (ancienne `github_releases()`) : `https`, hôte exact, port par défaut, aucun identifiant, chemin sous `/api/releases/`. Elle filtre l'installateur du client ET la cible de l'agent proposée par le client (`agent_update::validate_target`). Rien ne se lit sur le disque ni dans l'interface : liste fermée, compilée. Elle ne vaut que pour le PREMIER saut (voir la décision 6).
3. **Format d'adresse d'un fichier de version** : `https://hearth.voikyrioh.fr/api/releases/{X.Y.Z}/{fichier}`, version en `X.Y.Z` strict, SANS `v` devant (l'étiquette Git `vX.Y.Z` reste, elle, avec `v`). Noms de fichiers : liste fermée de l'API (`release-files.ts` de `hearth-releases-api`). `cargo xtask client-manifest --url` exige exactement `{préfixe}{X.Y.Z}/Hearth_{X.Y.Z}_x64-setup.exe` ; `cargo xtask agent-manifest --url` exige exactement `{préfixe}{X.Y.Z}/hearth-agent-linux-x86_64`, `X.Y.Z` étant la version de l'AGENT (contrainte de `deploy/install.sh`, BR-INSTALL-013). Toute autre adresse (autre dossier, autre fichier, port, identifiant, requête, fragment, ancien format avec `v`) est refusée avant publication. `check_version` refuse aussi les zéros en tête, comme l'API.
4. **Vérification de la signature du client par `xtask`, alignée sur le client.** Le greffon appelle `PublicKey::verify(data, &signature, true)` (`tauri-plugin-updater-2.13.1/src/updater.rs:1648`) et `minisign-verify` 0.2.5 accepte alors une signature non pré-hachée (`lib.rs:365-369`). `release_core.rs::verify_signature` garde donc `allow_legacy = true` : elle n'est pas plus lâche que le client. `client-sign` signe toujours en pré-haché.
5. **Garde contre la resignature d'un numéro déjà étiqueté** : `client-release-check` refuse si l'étiquette `vX.Y.Z` existe déjà sur `origin` (`git ls-remote --exit-code --tags`, lecture seule). Sans réponse de `origin`, il refuse aussi. Un numéro déjà étiqueté n'est jamais resigné. L'étiquette est posée à la fin, à la main : la fenêtre entre la signature et l'étiquette (relances, essai qui signe puis échoue) n'est PAS couverte ; elle le sera par le refus de `release-upload` sur une version déjà déposée sur le serveur (HRT-51). La garde tourne dans le job `build`, avant l'approbation de `sign`.

6. **Redirections (reprend la décision d'ADR-0017, remplacée)** : au plus `MAX_REDIRECTS` = 5 sauts (le rang est compté, l'adresse de départ non), HTTPS à chaque saut (`DownloadPolicy::allows_redirect`, plus `https_only(true)` de reqwest), **aucune liste d'hôtes** sur les sauts suivants, adresses privées comprises côté client. Pourquoi : le filtre d'hôte ne vaut que pour le premier saut ; une liste sur les suivants rendrait MUETS les clients installés le jour où le service est déplacé (BR-UPDATE-008 : échec silencieux), sans correctif possible puisqu'il passerait par ce même canal. Conséquence assumée : face à un serveur compromis le filtre ne protège rien, seule la signature tient (contenu), avec HTTPS (transport) et la borne de 5 (boucles). Un manifeste redirigé vers un autre hôte HTTPS est suivi ; il ne peut faire installer ni un autre code, ni un ancien installateur (version signée).
7. **Asymétrie assumée avec `xtask`** : le client accepte une requête (`?x=1`) sur l'adresse de l'installateur et de la cible de l'agent (et un fragment sur l'installateur) ; `xtask` les refuse. Plus de marge côté client pour un futur nom de fichier, jamais plus lâche sur l'hôte ni sur le préfixe.
8. **Cas limites d'adresse figés par des tests** (`tests/update_domain.rs`, `tests/agent_update_domain.rs`, bibliothèque `url`) : refusés, le point final (`hearth.voikyrioh.fr.`), la barre inverse (`\@`, `\.`), un homoglyphe (IDN), un point encodé dans l'hôte ; acceptés, car ramenés à la même adresse par l'analyse, la casse (`HTTPS://HEARTH.VOIKYRIOH.FR`), `:443` explicite, le point idéographique `。` entre les étiquettes, une barre inverse dans le chemin sous le préfixe.

## Comment l'appliquer

- `xtask/src/release_core.rs::DOWNLOAD_PREFIX` et `update/domain.rs` (`DOWNLOAD_HOST` + `DOWNLOAD_PATH_PREFIX`) doivent rester d'accord.
- Ne jamais changer `FEED_URL` : changer de serveur = redéployer et déplacer le DNS, ou poser une redirection HTTPS (5 sauts permis).
- Les tests du client ont un serveur local ; les adresses de production ne sont éprouvées que par `DownloadPolicy::official_releases()`.

## Quand NE PAS l'appliquer / limites

- Le serveur n'est pas de confiance : la signature reste la seule barrière (clé du client avec `requireSignedVersion`, clé minisign de l'agent). Cet ADR ne change rien à ADR-0008, ADR-0014 ni aux décisions de sécurité d'ADR-0017 et d'ADR-0021 autres que l'hébergement.
- Dépendance au serveur de Voiky, acceptée : serveur éteint, le client échoue en silence (BR-UPDATE-007, 008) et réessaie le lendemain ; rien d'installé ne casse.
- `deploy/install.sh` pointe encore GitHub par défaut : réécrit par un autre ticket (pointeur de l'API).

## Ce qui devient faux ailleurs

- `docs/runbooks/publier-une-version-du-client.md` (lignes 7, 37, 72-80) décrit un flux et une adresse GitHub qui n'existent plus côté client : faux jusqu'à la PR 71.
- `docs/runbooks/mettre-a-jour-agent.md` (ligne 41) donne une commande `agent-manifest --url https://github.com/…` que `xtask` refuse : faux jusqu'à HRT-55.
- `.github/workflows/publish-client.yml` passe encore `--url` GitHub : ne pas le lancer avant la PR 71.

## Alternatives rejetées

- Rester sur les GitHub Releases : adresse compilée chez un tiers, refusé par le propriétaire.
- Un nom à deux niveaux ou un bucket joint directement : voir la conception (section 15).

## Conséquences

- Client : `update/domain.rs`, `update/feed.rs`, `update/mod.rs`, `agent_update/domain.rs` (commentaires, appel renommé) ; tests recalés.
- Outillage : `xtask` (`release_core.rs`, `client_release.rs`, `agent_release.rs`, `main.rs`).
- Règles : `BR-UPDATE-001`, `BR-UPDATE-004`, `BR-UPDATE-007`, `BR-UPDATE-010`, `BR-UPDATE-027` (historique daté du 2026-10-12).
- Docs : `CLAUDE.md`, `ARCHITECTURE.md`, `apps/desktop/ARCHITECTURE.md`, `docs/open-api/agent-update.md`.

## Références

- Conception : hub `contexts/hearth/conceptions/2026-10-09-distribution-scaleway.md`.
- Greffon : `tauri-plugin-updater` 2.13.1 ; `minisign-verify` 0.2.5.
