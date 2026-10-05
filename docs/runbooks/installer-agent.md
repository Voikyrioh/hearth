# Installer, réinstaller et désinstaller l'agent

L'agent s'installe en une commande sur un serveur Linux (x86_64 ou arm64). Il devient un service qui démarre avec la machine, avec son premier compte administrateur et son empreinte.

Règles : `docs/business-rules/BR-INSTALL-*.md`. Décision de service (root, durcissement, installation gérée) : `docs/adr/ADR-0012-service-systeme.md`.

## Prérequis

- Un accès shell avec les droits d'administration (`sudo`).
- Linux x86_64 ou arm64 ; systemd (sinon, installation gérée, voir plus bas).
- Le port 7341 libre (ou un autre, au choix) et 256 Mio libres sur le disque du dossier de données.
- Le binaire : fichier local (`cargo xtask agent` le construit dans `target/dist/hearth-agent`), ou une adresse. **La publication des versions n'existe pas encore** : sans `--binary` ni adresse, le téléchargement par défaut échoue proprement (« Aucune version de l'agent n'est publiée… ») et rien n'est installé.

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

```sh
sudo HEARTH_ADMIN_USER=marie HEARTH_ADMIN_PASSWORD='…' HEARTH_PORT=7341 \
  sh deploy/install.sh --binary ./hearth-agent --yes
```

- `HEARTH_ADMIN_USER` et `HEARTH_ADMIN_PASSWORD` (ou `HEARTH_ADMIN_PASSWORD_HASH`, un haché Argon2id au format PHC `$argon2id$v=19$m=…,t=…,p=…$sel$haché` : l'agent ne connaît alors jamais le mot de passe). Jamais en argument de commande.
- `HEARTH_PORT` ou `--port`. `--yes` : aucune question ; une valeur manquante arrête l'installation **avant toute écriture**.
- `HEARTH_SHA256` ou `--sha256` : somme attendue du binaire. Sans elle, le script le dit : le binaire n'est pas vérifié (la signature minisign arrivera avec ADR-0008).

Directement avec le binaire (sans le script) : `sudo ./hearth-agent install --yes` avec les mêmes variables.

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

## Installation gérée par le système (NixOS, autre gestionnaire que systemd)

```sh
sudo HEARTH_ADMIN_USER=marie HEARTH_ADMIN_PASSWORD='…' hearth-agent install --managed --yes
```

L'agent n'écrit **aucune unité** et ne copie pas son binaire : il crée le dossier de données, l'identité, le premier compte et `agent.toml` avec `managed = true` (pas de mise à jour automatique), puis le dit. Le système déclare et démarre le service (`hearth-agent serve`). Même option (ou `HEARTH_MANAGED=1`) pour `uninstall`.

## Si l'installation s'arrête

Un prérequis qui manque, une erreur ou Ctrl+C : l'agent annonce ce qui s'est passé et **défait ce qu'il a fait** ; la machine reste comme avant. Ce qui n'a pas pu être défait est listé : à retirer à la main.

| Message | Cause | Que faire |
|---|---|---|
| « Droits d'administration requis… » | pas root | relancer avec `sudo` (la commande exacte est affichée) |
| « Le port configuré est déjà utilisé… » | un autre processus écoute | `ss -ltnp \| grep 7341`, ou relancer avec `--port 7342` |
| « Cette architecture n'est pas prise en charge… » | ni x86_64 ni arm64 | pas d'agent pour cette machine |
| « Espace disque insuffisant… » | moins de 256 Mio libres | libérer de l'espace |
| « systemd est introuvable… » | pas de systemd | `--managed` |
| « Une installation est déjà en cours… » | une autre installation tient le verrou | attendre qu'elle se termine |
| « Une version plus récente… est déjà installée » | le binaire installé est plus récent | rien : l'agent ne rétrograde pas |
| « Aucune version de l'agent n'est publiée… » | pas de publication de versions | `--binary` ou `HEARTH_RELEASE_URL` |
| « Le téléchargement n'a pas abouti… » | réseau coupé, adresse injoignable | relancer (aucun cache partiel n'est utilisé) |
| « La somme SHA-256 du binaire ne correspond pas… » | binaire altéré ou mauvaise somme | vérifier la source, ne pas installer |

## Dépannage

- **Le service ne démarre pas** : `systemctl status hearth-agent` puis `journalctl -u hearth-agent -n 100 --no-pager`. L'agent refuse de démarrer si une partie de son identité manque alors que le certificat existe (il ne régénère jamais en silence : l'empreinte ne doit pas changer) ; voir `docs/business-rules/BR-INSTALL-004-empreinte-generee-une-fois.md`. Un dossier de données ouvert aux autres utilisateurs est refusé : `chmod 700 /var/lib/hearth`.
- **L'unité ne se charge pas (`status=226/NAMESPACE`)** : un chemin de `ReadWritePaths` n'existe pas (dossier de données supprimé à la main). Le recréer (`install` répare).
- **Vérifier que l'agent répond** : `curl -k https://127.0.0.1:7341/api/v1/hello`.
- **Comparer l'empreinte** : `sudo hearth-agent fingerprint` (8 groupes de 4) ; ou celle du certificat réellement servi : `openssl s_client -connect 127.0.0.1:7341 </dev/null 2>/dev/null | openssl x509 -outform DER | sha256sum` (les 32 premiers caractères hexadécimaux).
- **Mot de passe oublié** : `docs/runbooks/recuperer-acces-administrateur.md`.
- **Voir ce que l'agent a installé** : `systemctl cat hearth-agent`, `ls -l /usr/local/bin/hearth-agent /etc/hearth /var/lib/hearth`.

## Vérifier l'installation de bout en bout (développement)

`cargo xtask e2e-install` construit le binaire statique, démarre un conteneur Debian avec systemd et rejoue tout ce runbook (installation par le script lu depuis un tube, réinstallation, désinstallation avec conservation puis purge, refus, retour en arrière, installation gérée). Docker est requis. `cargo xtask shellcheck` contrôle les scripts.
