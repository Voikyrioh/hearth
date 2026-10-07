# Installer, réinstaller et désinstaller l'agent

L'agent s'installe en une commande sur un serveur Linux x86_64 (arm64 viendra plus tard : aucun binaire arm64 n'est construit, l'installation le refuse clairement). Il devient un service qui démarre avec la machine, avec son premier compte administrateur et son empreinte.

Règles : `docs/business-rules/BR-INSTALL-*.md`. Décision de service (root, durcissement, installation gérée) : `docs/adr/ADR-0012-service-systeme.md`.

## Prérequis

- Un accès shell avec les droits d'administration (`sudo`).
- Linux x86_64 ; systemd (sinon, installation gérée, voir plus bas).
- Le port 7341 libre (ou un autre, au choix) et 256 Mio libres sur le disque du dossier de données.
- Le binaire : fichier local (`cargo xtask agent` le construit dans `target/dist/hearth-agent`, avec sa somme SHA-256 dans la CI), ou une adresse **HTTPS** avec sa somme SHA-256. **La publication des versions n'existe pas encore** : sans `--binary` ni adresse, le téléchargement par défaut échoue proprement et rien n'est installé.
- Le dossier de données (`/var/lib/hearth`) : absolu, chemin simple, appartenant à root et fermé aux autres (0700) s'il existe déjà ; sinon l'installation refuse avant d'écrire.

Chemins fixes : binaire `/usr/local/bin/hearth-agent`, configuration `/etc/hearth/agent.toml`, données `/var/lib/hearth/` (base, certificat, clé), unité `/etc/systemd/system/hearth-agent.service`.

## Installation interactive

```sh
sudo sh deploy/install.sh --binary ./hearth-agent
# ou, avec une adresse :
curl -fsSL <adresse-du-script> | sudo HEARTH_RELEASE_URL=<adresse-du-binaire> HEARTH_SHA256=<somme> sh
```

Lu depuis un tube (`curl … | sh`), le script garde les questions sur le terminal (`/dev/tty`). Il demande :

1. le numéro de port (entrée vide : 7341) ;
2. le nom du compte administrateur (3 à 32 caractères : minuscules, chiffres, `-`, `_`) ;
3. le mot de passe (12 caractères au moins, une majuscule, une minuscule, un chiffre), saisi sans écho, puis confirmé.

À la fin : « Installation réussie… », l'**empreinte du serveur** (8 groupes de 4) et l'adresse à saisir dans le client. **Note l'empreinte** : le client te la montrera à la première connexion, compare-les.

## Installation sans question

Le mot de passe du premier compte ne se met **jamais** sur une ligne de commande : un argument de `sudo` ou d'un script est lisible par tous les utilisateurs (`ps`), écrit dans le journal de `sudo` et dans l'historique du shell. Trois voies :

1. **Interactive** (ci-dessus) : saisie sans écho.
2. **Haché** (recommandée pour l'automatisation) : fabrique le haché Argon2id sur ta machine, mot de passe saisi sans écho, puis fournis-le par l'environnement :

   ```sh
   HEARTH_ADMIN_PASSWORD_HASH="$(hearth-agent hash-password --user marie)"; export HEARTH_ADMIN_PASSWORD_HASH
   HEARTH_ADMIN_USER=marie; export HEARTH_ADMIN_USER
   sudo --preserve-env=HEARTH_ADMIN_USER,HEARTH_ADMIN_PASSWORD_HASH \
     sh deploy/install.sh --binary ./hearth-agent --yes
   ```

   Le haché doit être un Argon2id au format PHC, version 19, mémoire de 19 à 256 Mio (`m=19456` à `262144`), 2 à 10 itérations, parallélisme de 1 à 4, sel d'au moins 16 octets et sortie d'au moins 32 : un haché trop faible ou trop gourmand est refusé avant toute écriture. L'agent ne connaît alors jamais le mot de passe.
3. **Variable lue au clavier** : `read -rs HEARTH_ADMIN_PASSWORD; export HEARTH_ADMIN_PASSWORD`, puis le même `sudo --preserve-env=…` (avec `HEARTH_ADMIN_PASSWORD` dans la liste).

L'installateur ne passe jamais ces variables à ses sous-processus (`df`, `systemctl`, l'ancien binaire) et le service, lancé par systemd, ne les reçoit pas.

- `HEARTH_PORT` ou `--port`. `--yes` : aucune question ; une valeur manquante arrête l'installation **avant toute écriture**.
- **Téléchargement** : `--url` (ou `HEARTH_RELEASE_URL`) en **HTTPS seulement** (redirections bornées et en HTTPS seulement). La somme SHA-256 est **obligatoire** : `--sha256` / `HEARTH_SHA256`, ou publiée à côté (`ADRESSE.sha256`, même origine). Sans somme, rien n'est téléchargé.
- **Fichier local** : `--binary` (chemin résolu en absolu, jamais cherché dans le `PATH`). Sans `--sha256`, le script annonce clairement, avant de le lancer en root, que le binaire n'est pas vérifié. La signature minisign arrivera avec ADR-0008.

Directement avec le binaire (sans le script) : `sudo --preserve-env=… ./hearth-agent install --yes` avec les mêmes variables.

## Réinstaller, mettre à niveau

Relancer la même commande avec le nouveau binaire. L'agent détecte l'installation :

- **comptes, journal, certificat (donc empreinte) et configuration sont conservés** ; aucun compte n'est redemandé ;
- même version et mêmes octets, service en marche : rien n'est touché, le service n'est pas interrompu ;
- sinon le binaire est remplacé (écriture atomique) et le service redémarre ; les clients connectés se reconnectent seuls ;
- installation abîmée (binaire, unité ou données manquants) : ce qui manque est refait, les données restent ;
- changer de port : modifier `/etc/hearth/agent.toml` à la main puis `sudo systemctl restart hearth-agent` (l'installation conserve le port existant).

## Désinstaller

```sh
sudo hearth-agent uninstall            # demande : conserver ou supprimer les données
sudo hearth-agent uninstall --keep-data --yes   # comptes, journal, empreinte, configuration conservés
sudo hearth-agent uninstall --purge --yes       # plus aucune trace
```

Après `--keep-data`, une nouvelle installation retrouve comptes et empreinte. Après `--purge`, la machine ne garde rien de l'agent (le verrou d'installation vit dans `/run`, volatil).

**La purge ne supprime que les fichiers que Hearth connaît** (`cert.pem`, `key.pem`, `install_id`, `identity.lock`, `request_fingerprint.key` (le secret d'empreinte, `secret-d-empreinte.md`), `hearth.db` et ses fichiers compagnons, qui contiennent comptes et journal), puis le dossier s'il est vide ; de même pour la configuration et son dossier. Un dossier de données partagé avec autre chose garde ce qui n'est pas à Hearth : ces fichiers sont listés, jamais touchés. Aucun `rm -r` n'est fait sur un chemin venu de la configuration.

## Installation gérée par le système (NixOS, autre gestionnaire que systemd)

```sh
sudo --preserve-env=HEARTH_ADMIN_USER,HEARTH_ADMIN_PASSWORD_HASH hearth-agent install --managed --yes
```

L'agent n'écrit **aucune unité** et ne copie pas son binaire : il crée le dossier de données, l'identité, le premier compte et `agent.toml` avec `managed = true` (pas de mise à jour automatique), puis le dit. Le système déclare et démarre le service (`hearth-agent serve`). Même option (ou `HEARTH_MANAGED=1`) pour `uninstall`.

## Si l'installation s'arrête

Un prérequis qui manque, une erreur ou Ctrl+C : l'agent annonce ce qui s'est passé et **défait ce qu'il a fait** ; la machine reste comme avant. Ce qui n'a pas pu être défait est listé : à retirer à la main.

| Message | Cause | Que faire |
|---|---|---|
| « Droits d'administration requis… » | pas root | relancer avec `sudo` (la commande exacte est affichée) |
| « Le port configuré est déjà utilisé… » | un autre processus écoute | `ss -ltnp \| grep 7341`, ou relancer avec `--port 7342` |
| « Cette architecture n'est pas prise en charge… » | pas x86_64 (arm64 inclus pour l'instant) | pas d'agent pour cette machine |
| « L'identité du serveur est incomplète… » | certificat, clé et identifiant d'installation ne sont pas tous là | rien n'est touché ; restaurer depuis une sauvegarde, ou supprimer les trois fichiers à la main si une nouvelle empreinte est acceptable |
| « Le dossier de données n'appartient pas à root » ou « est ouvert à d'autres utilisateurs » | droits du dossier existant | `chown root` / `chmod 700`, ou autre dossier |
| « L'adresse … n'est pas en HTTPS » / « Aucune somme SHA-256 n'est fournie ni publiée… » | téléchargement non vérifiable | adresse HTTPS et `--sha256` |
| « …hors des bornes acceptées » | haché fourni trop faible ou trop gourmand | `hearth-agent hash-password` |
| « Espace disque insuffisant… » | moins de 256 Mio libres | libérer de l'espace |
| « systemd est introuvable… » | pas de systemd | `--managed` |
| « Une installation est déjà en cours… » | une autre installation tient le verrou | attendre qu'elle se termine |
| « Une version plus récente… est déjà installée » | le binaire installé est plus récent | rien : le script n'installe jamais une version plus ancienne que le binaire installé |
| « Le chemin du dossier de données existe mais n'est pas un dossier » | un fichier occupe l'emplacement | le retirer, ou autre dossier |
| « Aucune version de l'agent n'est publiée… » | pas de publication de versions | `--binary` ou `HEARTH_RELEASE_URL` |
| « Le téléchargement n'a pas abouti… » | réseau coupé, adresse injoignable | relancer (aucun cache partiel n'est utilisé) |
| « La somme SHA-256 du binaire ne correspond pas… » | binaire altéré ou mauvaise somme | vérifier la source, ne pas installer |

## Réinstaller une version plus ancienne après `uninstall --keep-data`

Le refus de rétrograder ne porte que sur le **binaire installé**. Après `uninstall --keep-data`, le binaire n'est plus là, mais la base reste, et une version plus récente a pu la migrer. Réinstaller dessus une version plus ancienne n'est pas refusé par `install` : c'est le **démarrage** de l'agent qui échoue (SQLx refuse une base dont des migrations lui sont inconnues). L'installation ne reçoit aucune réponse, s'annule et défait tout (le binaire d'avant, l'unité, l'activation au démarrage) ; la base n'est pas touchée. Pour revenir en arrière malgré tout : réinstaller la version qui avait migré la base, ou `uninstall --purge` (les comptes et le journal sont perdus).

## Variables d'environnement et messages trompeurs

- **`install.sh` hérite de l'environnement de `sudo`** : les variables `HEARTH_ADMIN_USER`, `HEARTH_ADMIN_PASSWORD` et `HEARTH_ADMIN_PASSWORD_HASH` qui traînent dans le terminal sont lues par l'installation. Un mot de passe oublié dans l'environnement crée donc un premier compte sans qu'il soit demandé. Vérifier avec `env | grep HEARTH_` avant de lancer. Les sous-processus de l'installation (`systemctl`, `df`, l'ancien binaire) ne reçoivent jamais ces variables.
- **Somme SHA-256** : `install.sh` dit d'où elle vient. Donnée par `--sha256`, elle protège contre un binaire abîmé **et** contre une origine compromise. Publiée à côté du binaire (`ADRESSE.sha256`, même origine), elle ne protège que d'un téléchargement abîmé : le script le dit.
- **Redirections** : le script ne suit une redirection que si elle reste en HTTPS (avec `curl` comme avec `wget`).

## Dépannage

- **Le service ne démarre pas** : `systemctl status hearth-agent` puis `journalctl -u hearth-agent -n 100 --no-pager`. L'agent refuse de démarrer si une partie de son identité manque alors que le certificat existe (il ne régénère jamais en silence : l'empreinte ne doit pas changer) ; voir `docs/business-rules/BR-INSTALL-004-empreinte-generee-une-fois.md`. Un dossier de données ouvert aux autres utilisateurs est refusé : `chmod 700 /var/lib/hearth`.
- **L'unité ne se charge pas (`status=226/NAMESPACE`)** : un chemin de `ReadWritePaths` n'existe pas (dossier de données supprimé à la main). Le recréer (`install` répare).
- **Vérifier que l'agent répond** : `curl -k https://127.0.0.1:7341/api/v1/hello`.
- **Comparer l'empreinte** : `sudo hearth-agent fingerprint` (8 groupes de 4) ; ou celle du certificat réellement servi : `openssl s_client -connect 127.0.0.1:7341 </dev/null 2>/dev/null | openssl x509 -outform DER | sha256sum` (les 32 premiers caractères hexadécimaux).
- **Mot de passe oublié** : `docs/runbooks/recuperer-acces-administrateur.md`.
- **Voir ce que l'agent a installé** : `systemctl cat hearth-agent`, `ls -l /usr/local/bin/hearth-agent /etc/hearth /var/lib/hearth`.

## Vérifier l'installation de bout en bout (développement)

`cargo xtask e2e-install` construit le binaire statique, démarre un conteneur Debian avec systemd et rejoue tout ce runbook (installation par le script lu depuis un tube en HTTPS, réinstallation, mise à niveau depuis une autre version, désinstallation avec conservation puis purge d'un dossier partagé, refus, erreur tardive et SIGHUP avec retour en arrière complet, installation interactive sur un terminal, installation gérée). Docker est requis. `cargo xtask shellcheck` contrôle les scripts.
