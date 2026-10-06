# Mettre l'agent à jour à distance

Procédure d'exploitation de la mise à jour de l'agent (HRT-17). Règles : `BR-UPDATE-011` à `BR-UPDATE-019`, `BR-UPDATE-024`, `BR-UPDATE-027` à `BR-UPDATE-029`. Décisions : ADR-0008, ADR-0014. Contrat : `docs/open-api/agent-update.md`.

## En bref

Un administrateur demande la mise à jour depuis le client (écran livré avec HRT-17 côté client). L'agent télécharge le binaire en HTTPS, vérifie la somme SHA-256 et la signature minisign **avant d'écrire quoi que ce soit**, lance un superviseur détaché, qui arrête le service, échange les binaires, redémarre et attend 60 secondes que le nouvel agent réponde avec la nouvelle version et le même certificat. Sinon, l'ancien binaire revient, **identique octet pour octet**. Comptes, sessions, journal et empreinte ne sont pas touchés.

**Pas de proxy** : le serveur télécharge directement, il n'utilise jamais `HTTPS_PROXY` ni `ALL_PROXY` (BR-UPDATE-027 : un proxy résoudrait le nom lui-même et contournerait le filtre des adresses). Un serveur qui ne sort que par un proxy sortant ne peut pas se mettre à jour à distance (`unreachable`) : c'est assumé, mets-le à jour par `install.sh --binary`.

Pas de mise à jour à distance si l'installation est gérée par le système (`--managed`, NixOS) ou sans systemd : mettre à jour par la configuration du système, ou par `install.sh --binary` (BR-INSTALL-007).

## Prérequis, une fois : la clé de signature

La clé publique est **embarquée** dans l'agent (`crates/hearth-agent/update-key.pub`). La clé du dépôt n'a **aucune clé secrète** : tant que tu n'as pas généré la tienne et reconstruit l'agent, aucune mise à jour n'est acceptée.

```sh
minisign -G -p update-key.pub -s ~/.hearth/update.key     # une fois ; la clé secrète reste chez toi, jamais dans le dépôt
cp update-key.pub crates/hearth-agent/update-key.pub      # puis reconstruire et réinstaller l'agent (cargo xtask agent)
```

Changer la clé = réinstaller à la main les agents existants (ils n'acceptent que l'ancienne).

## Publier une version

1. Construire : `cargo xtask agent` (binaire statique `target/dist/hearth-agent`, taille et SHA-256 affichés).
2. Signer : `minisign -S -s ~/.hearth/update.key -m hearth-agent -x hearth-agent.minisig`.
3. Héberger le binaire en **HTTPS** (TLS 1.3, certificat reconnu par le système du serveur), à une adresse sans identifiant. Le client lit la cible dans `agent.json`, publié dans la MÊME release que `latest.json` (voir « Publier la cible pour le client » ci-dessous), et la transmet à l'agent ; à la main : `POST /api/v1/agent/update` avec `{ version, url, signature, sha256 }`.

## Publier la cible pour le client (`agent.json`, ADR-0021)

Le client (HRT-17, lot interface) propose « Mettre à jour l'agent » quand `agent.json`, dans la dernière release publiée, annonce une version STRICTEMENT plus récente que celle de l'agent. Il le lit dans la même tentative que `latest.json` (au plus une par 24 h et par client, ou « Vérifier maintenant »). Rien n'est publié automatiquement : tout se fait à la main, avec TA clé secrète, qui ne quitte jamais ta machine.

1. Construire et signer le binaire (étapes 1 et 2 ci-dessus). Le nom du binaire publié est `hearth-agent-linux-x86_64` (copie de `target/dist/hearth-agent`), la signature `hearth-agent-linux-x86_64.minisig`.
2. Fabriquer `agent.json` (aucun secret ; la signature est vérifiée contre `crates/hearth-agent/update-key.pub`, la clé embarquée dans l'agent : une signature que l'agent refuserait n'est pas publiée) :
   ```sh
   # --version DOIT être la version du dépôt ([workspace.package] de Cargo.toml) : celle du binaire construit
   cargo xtask agent-manifest --version X.Y.Z \
     --binary-file hearth-agent-linux-x86_64 --signature-file hearth-agent-linux-x86_64.minisig \
     --url https://github.com/Voikyrioh/hearth/releases/download/vX.Y.Z/hearth-agent-linux-x86_64 \
     --date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --out agent.json
   ```
3. Joindre les TROIS fichiers (`hearth-agent-linux-x86_64`, `.minisig`, `agent.json`) à la release `vX.Y.Z` de `Voikyrioh/hearth`, avec `latest.json` du client (flux `publish-client`, runbook `publier-une-version-du-client.md`), AVANT de la publier : `gh release upload vX.Y.Z hearth-agent-linux-x86_64 hearth-agent-linux-x86_64.minisig agent.json` sur le brouillon. Le flux `publish-client` ne joint pas ces fichiers et ne signe jamais l'agent.
   Avant de publier, vérifier que les TROIS fichiers sont bien sur le brouillon : `gh release view vX.Y.Z --json assets --jq '.assets[].name'` doit lister `hearth-agent-linux-x86_64`, `hearth-agent-linux-x86_64.minisig` et `agent.json` (sans `agent.json`, le client ne propose rien ; oublié sur une release « dernière », il retire la proposition à tous). Le numéro de l'adresse du binaire est celui de l'AGENT, le nom de la release (`vX.Y.Z`) celui qui porte le client : ils peuvent différer, `agent-manifest` exige que l'adresse commence par `…/releases/download/v{version}/` avec `--version` donné.
4. Toute release qui doit rester « dernière » porte `latest.json` ET `agent.json` : l'adresse `releases/latest/download/agent.json` suit la dernière release publiée. Une release du client seul (agent inchangé) reprend l'`agent.json` précédent (`gh release download vANCIENNE --pattern agent.json`, puis `gh release upload` sur le brouillon ; même version d'agent : rien de « disponible » pour les agents à jour) ; sans `agent.json`, le client ne propose plus rien pour l'agent et reste utilisable.
5. Contrôler après publication : « Vérifier maintenant » dans les réglages d'un client dont l'agent est plus ancien fait apparaître « Mise à jour disponible » sur la carte du serveur ; un agent déjà à jour ne montre rien. Le client ne propose jamais une version plus ancienne ou égale, ni une adresse qui n'est pas HTTPS, publique et dans les releases du dépôt.

## Suivre une mise à jour

- Depuis le client : carte « État du serveur : {nom} » des réglages, étapes `Téléchargement X %`, `Vérification…`, `Installation…`, `Redémarrage…`, `Contrôle…`.
- Sur le serveur : `journalctl -u hearth-agent -u hearth-agent-update -f` (le superviseur est l'unité transitoire `hearth-agent-update`).
- État : `GET /api/v1/agent/update` (en cours, étape) et `GET /api/v1/agent/update/last` (résultat, lisible après un redémarrage). Fichiers : `/var/lib/hearth/update/` (`last.json`, `state.json`, `job.json`).
- Journal d'activité : action « Mise à jour de l'agent », avec qui l'a demandée et le résultat.

## Résultats et que faire

| Résultat | Sens | Que faire |
|---|---|---|
| `succeeded` | nouvel agent en place | rien. L'ancien binaire est retiré. |
| `rolled_back` / `no_answer` | le nouvel agent n'a pas répondu en 60 s : l'ancien binaire est revenu et tourne | lire `journalctl -u hearth-agent` à l'heure de la tentative (le nouvel agent a loggé pourquoi il ne démarre pas). Si le démarrage est seulement lent (> 60 s), relancer la mise à jour à la main après correction. |
| `rolled_back` / `identity_changed` | le nouvel agent présentait un autre certificat | ne pas forcer : l'empreinte ne doit jamais changer (BR-INSTALL-004) ; vérifier le dossier de données de la nouvelle version. |
| `failed` / `unreachable` | le serveur n'a pas joint l'adresse (pas d'Internet, DNS, certificat inconnu) | tester `curl -v <url>` depuis le serveur ; l'agent continue de tourner. |
| `failed` / `download_failed`, `bad_checksum`, `bad_signature`, `bad_binary` | fichier absent, coupé, altéré, d'une autre clé, ou qui n'annonce pas la version visée | vérifier la publication ; rien n'a été écrit. |
| `failed` / `staging`, `swap`, `supervisor_launch` | disque plein, droits, `systemd-run` absent ; pour `swap` aussi : pas assez de place pour copier la base (deux fois sa taille, plus 1 Mio) **ou espace libre impossible à mesurer** (`df` absent ou illisible : refus, jamais « assez de place ») | `df -h /var/lib/hearth`, `systemctl status`, `journalctl -u hearth-agent-update` ; l'agent n'a pas changé. Le contrôle de place précède l'arrêt du service ; la place mesurée est celle d'un utilisateur ordinaire (`df`) : un disque réduit à la réserve de root est refusé par prudence. |
| `failed` / `interrupted` | la mise à jour a été laissée en cours puis conclue au démarrage, **ou** une version tierce a été posée à la main par-dessus (voir « Reprise vers une troisième version ») | relire `GET /agent/update/last`. Si la version visée est vide (`version_unknown`), une trace de travail était illisible : voir « Reprise à la main ». |
| `failed` / `rollback_failed` | le retour arrière lui-même a échoué | voir ci-dessous. |

## Mise à jour interrompue (redémarrage du serveur, agent ou superviseur tué)

Au démarrage, l'agent conclut tout seul ce qui était en cours (BR-UPDATE-028) : avant l'échange des binaires, résultat `failed` / `interrupted` et dépôt nettoyé ; après l'échange, un superviseur de reprise contrôle le binaire en place et, s'il ne tient pas, remet l'ancien binaire **et la base d'avant** (BR-UPDATE-029). Le résultat se lit comme les autres (`GET /agent/update/last`) et est au journal. Cas que l'agent ne peut pas conclure : le nouveau binaire ne démarre pas du tout (personne ne tourne) : voir « Reprise à la main ».

## Reprise à la main (`rollback_failed`, ou agent mort après une mise à jour)

L'ancien binaire est gardé à côté du binaire installé tant que la mise à jour n'est pas conclue :

```sh
ls -l /usr/local/bin/.hearth-agent.previous            # s'il existe, c'est l'ancien binaire
systemctl stop hearth-agent
cp -p /usr/local/bin/.hearth-agent.previous /usr/local/bin/hearth-agent
systemctl start hearth-agent && curl -sk https://127.0.0.1:7341/api/v1/hello
```

Après la reprise à la main, **redémarre l'agent** (`systemctl restart hearth-agent`) : il constate que la version d'avant tourne, conclut la mise à jour en `rolled_back`, retire la sauvegarde de l'ancien binaire et la copie de la base, **sans jamais toucher à la base** (les comptes et le journal écrits depuis sont gardés). Ne remets la copie de la base que si l'agent d'avant refuse de démarrer.

Si la base a été migrée par la nouvelle version, remettre aussi sa copie d'avant l'échange (`/var/lib/hearth/update/hearth.db.before`, et `hearth.db-wal.before` s'il existe), service arrêté : `cp -p /var/lib/hearth/update/hearth.db.before /var/lib/hearth/hearth.db` et supprimer `hearth.db-wal` et `hearth.db-shm`. Sans cette copie (retour arrière déjà fait par le superviseur : elle est retirée), l'ancien binaire refuse de démarrer sur une base migrée (« migration inconnue » dans `journalctl -u hearth-agent`) : remettre la **nouvelle** version (`install.sh --binary`). Une mise à jour interrompue par un redémarrage du serveur : relire `/api/v1/agent/update` ; si `in_progress` reste vrai plus de quelques minutes, `systemctl status hearth-agent-update` puis `systemctl stop hearth-agent-update`.

## Reprise vers une troisième version (ni l'ancienne ni la nouvelle)

Cas : la mise à jour de 0.1.0 vers 0.2.0 est restée en cours (nouvel agent muet, superviseur tué, `rollback_failed`) et tu poses à la main une version 0.3.0, qui n'est ni l'une ni l'autre (`install.sh --binary`, ou une copie du binaire).

Ce que l'agent fait au démarrage suivant (BR-UPDATE-028) : il voit qu'une version qui n'est ni la visée ni celle d'avant tourne. Il **ne lance aucune reprise** et ne remet **ni l'ancien binaire ni la copie de la base** : la mise à jour est conclue `failed` / `interrupted` (entrée au journal d'activité, au nom de qui l'avait demandée), et il **retire** la sauvegarde de l'ancien binaire (`.hearth-agent.previous`), la copie de la base (`update/hearth.db.before`, `update/hearth.db-wal.before` s'il existe) et les traces (`update/job.json`, `update/state.json`). La base vivante n'est jamais touchée : comptes, sessions et journal écrits depuis l'échange sont gardés.

Conséquence : une fois ce démarrage passé, il n'y a plus de chemin de retour automatique vers 0.1.0 ni de copie de la base d'avant l'échange.

Marche à suivre :

1. **Avant de poser la 0.3.0**, si tu veux garder un retour possible : service arrêté, copie `hearth.db*` du dossier de données et, si tu veux l'état d'avant l'échange, `update/hearth.db.before` et `.hearth-agent.previous`, ailleurs que dans `update/` (ils seront supprimés).
2. Pose la 0.3.0 (`install.sh --binary`) et laisse l'agent démarrer.
3. Vérifie : `curl -sk https://127.0.0.1:7341/api/v1/hello` (version 0.3.0), `GET /api/v1/agent/update/last` (`failed` / `interrupted`), et que `update/` ne contient plus ni `job.json`, ni `state.json`, ni `hearth.db.before`.
4. Si la 0.3.0 refuse de démarrer sur la base (« migration inconnue » dans `journalctl -u hearth-agent`) : remets la copie que tu as gardée à l'étape 1, service arrêté, supprime `hearth.db-wal` et `hearth.db-shm`, puis redémarre.

Par `install.sh --binary`, l'installation utilise le même chemin de sauvegarde et le retire à la fin : le démarrage suivant conclut pareil (`failed` / `interrupted`, base intacte, copies retirées), par un autre chemin du code. `ForeignVersion` ne sert que pour un binaire copié à la main.

Tu peux aussi supprimer toi-même les traces avant de poser la version tierce (les cinq fichiers ci-dessus) : l'agent n'aura alors rien à conclure.

## Reprise restée sans résultat (`rollback_failed` après une reprise)

Au démarrage, si la mise à jour est restée en cours, l'agent tente **une seule** reprise automatique par échange. Si le superviseur de reprise ne conclut rien, ou si des numéros de version du travail sont illisibles, l'agent **ne relance rien** : résultat `failed` / `rollback_failed` (version « inconnue » si la trace est illisible), au journal d'activité. Les **copies sont gardées** (`.hearth-agent.previous`, `update/hearth.db.before`) ; seules `job.json` et `state.json` sont retirées. Reprise à la main : « Reprise à la main » ci-dessus.

Note : après un retour à la main vers un agent plus ancien, `update/last.json` reste lisible par lui (la version inconnue y est une chaîne vide, jamais `null`).

## Dépannage du lancement

- `supervisor_launch` immédiat : `systemd-run` absent du `PATH` du service, ou **dossier de données monté `noexec`** (le superviseur et le binaire déposé s'exécutent depuis `/var/lib/hearth/update/`) ; `journalctl -u hearth-agent` donne la cause. Remonter le dossier sans `noexec`, ou mettre à jour par `install.sh --binary`.
- `bad_binary` alors que le fichier est bon : même cause (le binaire ne peut pas s'exécuter), ou version annoncée différente de celle demandée.
- `unreachable` : l'adresse doit être publique et en HTTPS (adresses de bouclage, privées et lien-local refusées, aussi après résolution du nom, BR-UPDATE-027).

## Limites connues

- L'unité systemd n'est pas réécrite par une mise à jour : si une nouvelle version change le durcissement, lancer ensuite `sudo hearth-agent install` (réparation, données conservées).
- La mise à jour **vers une version plus ancienne** est refusée (`VALIDATION_ERROR`, champ `version`), et le client ne la propose ni ne l'envoie jamais.
- **Agent trop ancien pour le client** (versions d'interface incompatibles, BR-CONN-014) : l'agent répond `426` à toute route sauf `/hello`, donc le client ne peut pas le mettre à jour ; passer par `install.sh --binary` ou la configuration du système. Le client dit quoi faire (BR-UPDATE-020, 021).
- **Redémarrage long** : le client attend le nouvel agent 2 minutes au plus (« Reconnexion… », sans alarme) ; au-delà, le lien passe « Hors ligne » comme une panne ordinaire (ADR-0021).
- Un serveur de versions qui ne parle que TLS 1.2 est refusé (ADR-0014).
- **Double panne** : si le superviseur est tué après l'échange **et** que le nouveau binaire ne démarre pas du tout, personne ne tourne pour conclure et le service reste à terre : reprise à la main ci-dessus (copier `.hearth-agent.previous`, puis la copie de la base si la base a été migrée). Une reprise automatique est à l'étude (ADR-0014, section du 2026-10-06).
- À vérifier sur la vraie machine (NixOS avec carte NVIDIA) : voir `contexts/hearth/sessions/2026-10-04-hearth-creation/tasks/T20-hrt-17-maj-agent.md`. En installation gérée, la mise à jour passe par la configuration NixOS, pas par cette procédure.

## Vérifier de bout en bout (développement)

`cargo xtask e2e-update` (Docker requis) : construit l'agent en 0.1.0, 0.2.0 et 0.2.1 avec une clé jetable (dans un dossier de cibles à part), démarre un conteneur Debian avec systemd et rejoue six scénarios : signature d'une autre clé refusée avant toute écriture et somme fausse, mise à jour réussie (comptes, journal et empreinte intacts), agent muet (retour automatique, binaire identique), deuxième demande refusée, **superviseur tué juste après l'échange** (le redémarrage conclut), **agent tué pendant le téléchargement** (conclu « interrompue »). Une dizaine de minutes la première fois. La construction de publication (`cargo xtask agent`) vérifie sa version, la clé embarquée et l'absence de notes de test (`hearth-agent build-info`).
