---
id: ADR-0014
titre: Mise à jour de l'agent à distance : superviseur détaché (systemd-run), minisign-verify, reqwest et rustls-native-certs côté agent
type: librairie
statut: acceptée
date: 2026-10-05
portee: projet
remplace: —
liens: [ADR-0003, ADR-0008, ADR-0009, ADR-0012, conception technique 2026-10-04 sections 3, 5, 7 et 9, HRT-17]
---

# ADR-0014 : Mise à jour de l'agent à distance

## Contexte

ADR-0008 fixe le principe (flux de versions signé, superviseur, retour arrière en 60 s). HRT-17 le réalise côté serveur. Trois contraintes qu'ADR-0008 ne tranchait pas : l'unité de l'agent est durcie (ADR-0012 : `PrivateTmp`, `ProtectSystem=full`, `systemctl stop` tue tout le groupe de contrôle), l'agent n'avait aucun client HTTP, et rien ne doit être écrit ni exécuté avant la vérification de la signature **et** de la somme. Voiky installera l'agent sur un serveur que personne ne surveille : une mise à jour ratée ne doit jamais le laisser sans agent.

## Décision

1. **Le superviseur est une copie de l'ancien binaire**, déposée dans `<données>/update/supervisor` et lancée par `hearth-agent update-supervise --job <job.json>` (sous-commande cachée). Pas un second binaire à publier : le superviseur est exactement le code de l'agent qui l'a lancé, et il n'est pas touché par l'échange du binaire installé.
2. **Il est lancé par `systemd-run`** (unité transitoire `hearth-agent-update`, `--collect`), donc **hors du groupe de contrôle du service** : `systemctl stop hearth-agent` ne le tue pas (ADR-0012 le rappelait). Un lancement « détaché » par un double `fork` ne survivrait pas à l'arrêt du service. Sans systemd, ou en installation gérée (`--managed`), l'agent **refuse** la mise à jour à distance (`409 MANAGED_INSTALL`). Un lanceur « processus détaché » existe pour le développement et les tests (`Launcher::Detached`), jamais en production.
3. **Les fichiers de la mise à jour vivent dans `<données>/update/`** (binaire déposé, copie du superviseur, travail, étape, dernier résultat, verrou), pas dans `/tmp` : l'unité a `PrivateTmp=yes`, le superviseur ne verrait pas un fichier de `/tmp`, et seul le dossier de données est écrivable par l'agent. Dossier 0700, fichiers 0600 (0700 pour les binaires), écritures par fichier voisin puis renommage. Tous les noms sont dans `domain/install/files.rs`, **source unique** de la désinstallation (BR-INSTALL-011) : un nom écrit est un nom purgé.
4. **Un verrou de fichier** (`update/lock`, `File::try_lock` de la bibliothèque standard, sans `unsafe`) dit « un superviseur travaille ». Le système le relâche si le superviseur meurt : pas de verrou orphelin. Il sert à refuser une deuxième mise à jour, y compris pour le nouvel agent qui vient de redémarrer pendant le contrôle (BR-UPDATE-012).
5. **Le résultat est un fichier** (`update/last.json`), écrit par le superviseur (ou par l'agent pour un échec avant l'échange). Le premier agent qui revient (le nouveau, ou l'ancien après retour arrière) l'écrit au journal d'activité **une fois** (`reported`) et l'annonce sur le flux. Pas de table SQLite : le superviseur est un processus séparé, parfois d'une autre version que celle de l'agent, il ne doit ni ouvrir ni migrer la base.
6. **Vérification** : téléchargement **en mémoire** (128 Mio au plus), somme SHA-256 puis signature minisign, **avant la première écriture**. La signature est contrôlée dès la demande (lisible, **faite par la clé embarquée** : identifiant de clé) : une signature d'une autre clé est refusée avant tout téléchargement. Après dépôt, le binaire est exécuté une fois (`--version`, 10 s) pour vérifier qu'il tourne sur cette machine et annonce la version visée ; c'est la seule exécution avant l'échange, après vérification.
7. **La clé publique est embarquée** : `crates/hearth-agent/update-key.pub`, copiée dans le binaire par `build.rs`. Aucune clé ne se lit sur le disque du serveur. La clé du dépôt est une clé **sans clé secrète** (la secrète a été jetée) : tant que Voiky n'a pas généré la sienne et reconstruit, aucune mise à jour ne peut être acceptée. Pour les tests de bout en bout, `HEARTH_UPDATE_PUBKEY_FILE` (clé jetable) et `HEARTH_AGENT_VERSION` (fabriquer une « version suivante ») sont lus **à la construction** ; une construction de publication ne les définit pas. La version est lue en un seul endroit (`build_info::VERSION`).
8. **Retour arrière exact** : l'ancien binaire est gardé à côté du binaire installé (`.hearth-agent.previous`, lien dur, écriture atomique : `InstallHost::{install_binary, restore_binary}`, déjà éprouvés par l'installation) et remis octet pour octet. L'unité, l'activation au démarrage et la configuration ne sont jamais touchées par une mise à jour : rien à rétablir d'autre que le binaire et l'état du service.
10. **Un travail laissé en cours est conclu au démarrage** (BR-UPDATE-028). La demande écrit tout de suite `update/state.json` (version, étape, demandeur) ; le superviseur y ajoute `job.json` puis la sauvegarde de l'ancien binaire. Au démarrage, `domain::update::classify_orphan` lit ces traces (sans superviseur vivant, après avoir lu le verrou **avant** le résultat) : avant l'échange, abandon `failed` / `interrupted` ; après l'échange, un superviseur de reprise (`Job::recover`) contrôle le binaire en place et, sinon, remet l'ancien. Limite : si le nouveau binaire ne démarre pas du tout après un superviseur tué, personne ne tourne pour conclure (runbook).
11. **Le retour arrière remet aussi la base** (BR-UPDATE-029) : copie de `hearth.db` service arrêté avant l'échange, remise avant l'ancien binaire. Plus de chemin où l'ancien binaire reste devant une base migrée ; ce que le nouvel agent a écrit pendant la fenêtre de contrôle est perdu (accepté).
12. **Adresse de téléchargement bornée** (BR-UPDATE-027) : HTTPS, adresses de bouclage, privées, lien-local et partagées refusées à la demande, à chaque redirection et **après résolution** (résolveur dédié). Les calculs lourds (somme de 128 Mio, dépôt, `--version`, `systemd-run`) passent par `spawn_blocking`. L'unité transitoire est lancée avec `--service-type=exec` et `NoNewPrivileges`, `PrivateTmp`, `ProtectHome`.
9. **Le nouvel agent est tenu pour bon** s'il répond à `GET /hello` avec la nouvelle version **et le même certificat** (empreinte de 32 octets, BR-UPDATE-018), dans les 60 s après le redémarrage ; une autre identité est refusée tout de suite.

## Dépendances ajoutées à l'agent

| Crate | Rôle | Pourquoi | Limites / quand ne pas l'utiliser |
|---|---|---|---|
| `minisign-verify` 0.3 | Vérifier la signature minisign (Ed25519) | Sans dépendance, pur Rust, vérification seule (jamais de clé secrète dans l'agent), format de Tauri | Ne signe pas : la signature de publication se fait avec l'outil `minisign` ; `verify` sans le mode « legacy ». |
| `reqwest` 0.13 (`rustls-no-provider`, sans `default-features`) | Télécharger le binaire en HTTPS (flux, redirections contrôlées, délais) | Déjà dans l'arbre par `hearth-link` (ADR-0011) : même version, même configuration rustls + `ring`, pas d'aws-lc ni d'OpenSSL | HTTP/1.1 seul, TLS 1.3 seul. Un serveur de versions qui ne parle que TLS 1.2 est refusé : choisir un hébergement TLS 1.3 (GitHub, Cloudflare, tout hébergeur actuel). |
| `rustls-native-certs` 0.8 | Autorités de certification du système | Une adresse de flux de versions se valide comme n'importe quel site, y compris un hébergement interne dont le certificat est dans le magasin du système ; binaire statique : les certificats se lisent sur le disque (`/etc/ssl/certs`, `SSL_CERT_FILE`) | Un système sans magasin de certificats ne peut pas télécharger (erreur `unreachable`). |
| `base64` 0.22 | Signature donnée en base64 (format de Tauri) et lecture de l'identifiant de clé | Standard | — |
| `minisign` 0.10 (dev-dépendance de l'agent, dépendance de `xtask`) | Signer dans les tests et dans `cargo xtask e2e-update` | Générer une clé jetable et signer sans installer `minisign` | Jamais dans le binaire de l'agent. |

## Quand NE PAS l'appliquer / limites

- Mise à jour d'un serveur sans accès à Internet : `unreachable`, l'agent continue ; mise à jour manuelle (`install.sh --binary`).
- Une migration de base livrée avec la nouvelle version s'applique au démarrage du nouvel agent. Le retour arrière remet la base d'avant l'échange (décision 11), puis l'ancien binaire : plus de service arrêté devant une base migrée. Ce que le nouvel agent a écrit dans la base pendant la fenêtre de contrôle est perdu au retour arrière.
- **Dette datée** : le sujet `update` du flux est un type à part (`stream::UpdateMessage`), pas une variante de `ServerMessage`, parce que `hearth-link` (HRT-10) fait un `match` exhaustif. À rentrer dans `ServerMessage` quand la bibliothèque de liaison est libre, avec une variante inconnue tolérée côté client pour que la prochaine ne casse rien (suivi dans la tâche T20).
- Risque accepté : un serveur de versions interne (LAN) n'est pas pris en charge (adresses privées refusées).
- Un agent plus lent à démarrer que 60 s est revenu en arrière à tort : relancer à la main (le runbook donne la commande).
- Une nouvelle version qui change le texte de l'unité (capacités, durcissement) ne le fait pas par la mise à jour : lancer ensuite `hearth-agent install` (réparation) pour la réécrire.

## Alternatives rejetées

- **Un second binaire `hearth-supervisor`** : à publier, signer et tenir à jour en plus de l'agent.
- **Un script shell** (comme l'esquisse d'ADR-0008) : pas de contrôle de l'empreinte ni de la version, pas de tests unitaires.
- **Double `fork` / `setsid`** pour détacher : tué par `systemctl stop` (même groupe de contrôle).
- **Conserver le résultat dans SQLite** : le superviseur ne doit pas toucher à la base.
- **Clé publique lue dans un fichier de configuration** : qui peut écrire sur le serveur ferait accepter son binaire.
- **Vérifier la signature sur le disque** : le binaire non vérifié serait écrit.

## Conséquences

- `cargo xtask e2e-update` rejoue sur une vraie machine systemd : signature d'une autre clé refusée avant toute écriture, mise à jour réussie (comptes, journal, empreinte intacts), agent muet (retour automatique, binaire identique), deuxième demande refusée.
- Générer la clé de publication, signer et publier : `docs/runbooks/mettre-a-jour-agent.md`.

## Références

- ADR-0008 (principe), ADR-0012 (unité durcie), `systemd-run(1)`, https://jedisct1.github.io/minisign/
