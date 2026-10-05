# Mettre l'agent à jour à distance

Procédure d'exploitation de la mise à jour de l'agent (HRT-17). Règles : `BR-UPDATE-011` à `BR-UPDATE-019`, `BR-UPDATE-024`. Décisions : ADR-0008, ADR-0014. Contrat : `docs/open-api/agent-update.md`.

## En bref

Un administrateur demande la mise à jour depuis le client (écran livré avec HRT-17 côté client). L'agent télécharge le binaire en HTTPS, vérifie la somme SHA-256 et la signature minisign **avant d'écrire quoi que ce soit**, lance un superviseur détaché, qui arrête le service, échange les binaires, redémarre et attend 60 secondes que le nouvel agent réponde avec la nouvelle version et le même certificat. Sinon, l'ancien binaire revient, **identique octet pour octet**. Comptes, sessions, journal et empreinte ne sont pas touchés.

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
3. Héberger le binaire en **HTTPS** (TLS 1.3, certificat reconnu par le système du serveur), à une adresse sans identifiant. Le client lit le flux de versions (`latest.json` : version, adresse, signature, somme) et transmet la cible à l'agent ; à la main : `POST /api/v1/agent/update` avec `{ version, url, signature, sha256 }`.

## Suivre une mise à jour

- Depuis le client : étapes `Téléchargement X %`, `Vérification…`, `Installation…`, `Redémarrage…`, `Contrôle…`.
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
| `failed` / `staging`, `swap`, `supervisor_launch` | disque plein, droits, `systemd-run` absent | `df -h /var/lib/hearth`, `systemctl status`, `journalctl` ; l'agent n'a pas changé. |
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

## Dépannage du lancement

- `supervisor_launch` immédiat : `systemd-run` absent du `PATH` du service, ou **dossier de données monté `noexec`** (le superviseur et le binaire déposé s'exécutent depuis `/var/lib/hearth/update/`) ; `journalctl -u hearth-agent` donne la cause. Remonter le dossier sans `noexec`, ou mettre à jour par `install.sh --binary`.
- `bad_binary` alors que le fichier est bon : même cause (le binaire ne peut pas s'exécuter), ou version annoncée différente de celle demandée.
- `unreachable` : l'adresse doit être publique et en HTTPS (adresses de bouclage, privées et lien-local refusées, aussi après résolution du nom, BR-UPDATE-027).

## Limites connues

- L'unité systemd n'est pas réécrite par une mise à jour : si une nouvelle version change le durcissement, lancer ensuite `sudo hearth-agent install` (réparation, données conservées).
- La mise à jour **vers une version plus ancienne** est refusée (`VALIDATION_ERROR`, champ `version`).
- Un serveur de versions qui ne parle que TLS 1.2 est refusé (ADR-0014).
- À vérifier sur la vraie machine (NixOS avec carte NVIDIA) : voir `contexts/hearth/sessions/2026-10-04-hearth-creation/tasks/T20-hrt-17-maj-agent.md`. En installation gérée, la mise à jour passe par la configuration NixOS, pas par cette procédure.

## Vérifier de bout en bout (développement)

`cargo xtask e2e-update` (Docker requis) : construit l'agent en 0.1.0, 0.2.0 et 0.2.1 avec une clé jetable (dans un dossier de cibles à part), démarre un conteneur Debian avec systemd et rejoue six scénarios : signature d'une autre clé refusée avant toute écriture et somme fausse, mise à jour réussie (comptes, journal et empreinte intacts), agent muet (retour automatique, binaire identique), deuxième demande refusée, **superviseur tué juste après l'échange** (le redémarrage conclut), **agent tué pendant le téléchargement** (conclu « interrompue »). Une dizaine de minutes la première fois. La construction de publication (`cargo xtask agent`) vérifie sa version, la clé embarquée et l'absence de notes de test (`hearth-agent build-info`).
