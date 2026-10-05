---
id: ADR-0017
titre: Mise à jour du client : greffon tauri-plugin-updater (API Rust seule), flux GitHub Releases, clé embarquée, état dans un fichier Rust
type: securite
statut: acceptée
date: 2026-10-05
portee: projet
remplace: —
liens: [ADR-0002, ADR-0008, ADR-0009, ADR-0010, ADR-0011, ADR-0014, conception technique 2026-10-04 sections 3, 7 et 10, HRT-16]
---

# ADR-0017 : Mise à jour du client

## Contexte

ADR-0008 fixe le principe (minisign, flux de versions) et ADR-0014 l'a réalisé pour l'agent. HRT-16 le réalise pour le client Windows : sans lui, chaque correction demanderait de réinstaller à la main. C'est aussi un canal d'exécution de code sur le PC de l'utilisateur : une mise à jour non vérifiée serait la pire faille du produit. Le « point ouvert » de la conception technique (où publier les versions) est tranché : **les GitHub Releases du dépôt public** `Voikyrioh/hearth`.

## Décision

1. **Greffon officiel `tauri-plugin-updater` 2.13, utilisé par son API RUST seulement.** Il fait le réseau, lit le manifeste statique, télécharge l'installateur en mémoire, vérifie la signature minisign et lance l'installateur NSIS (`/UPDATE`, mode passif, relance du client). Écrire ce chemin à la main (téléchargement, minisign-verify, lancement) aurait recopié du code sensible déjà éprouvé et maintenu. **Aucune permission du greffon n'est accordée à la WebView** (`capabilities/default.json` n'en contient aucune ; un test le vérifie) : ses commandes JavaScript (qui accepteraient une adresse, des en-têtes, un mandataire) restent inaccessibles. L'interface ne dispose que de nos quatre commandes typées (`get_update_state`, `check_for_updates`, `postpone_update`, `install_update`), qui ne prennent ni adresse, ni chemin, ni clé.
2. **Flux : `https://github.com/Voikyrioh/hearth/releases/latest/download/latest.json`**, constante de la compilation (`update/domain.rs::FEED_URL`), pas un réglage. `latest` pointe la dernière release PUBLIÉE (ni brouillon ni préversion). Le manifeste est celui du greffon (`version`, `notes`, `pub_date`, `platforms.windows-x86_64.{signature,url}`), fabriqué par `cargo xtask client-manifest` ; il pourra recevoir plus tard une section `agent` (ADR-0008) sans gêner le greffon, qui ignore les champs inconnus. L'installateur n'est téléchargé que depuis `github.com/Voikyrioh/hearth/releases/download/…` en HTTPS : la source est contrôlée AVANT d'afficher le bandeau et de retenir l'annonce (`DownloadPolicy`), et chaque REDIRECTION est contrôlée aussi (HTTPS à chaque saut, au plus 3 sauts, hôtes `github.com` et `*.githubusercontent.com` seulement ; `https_only` du client HTTP quand la source l'est). Hôtes réellement traversés, relevés par lecture des en-têtes d'une release publique d'un AUTRE dépôt (`cli/cli`, requêtes de lecture) : `github.com/…/releases/latest/download/x` puis `github.com/…/releases/download/vN/x` (302) puis `release-assets.githubusercontent.com` (302) ; le greffon applique cette règle au manifeste comme à l'installateur (`configure_client`). Le greffon refuse en plus tout flux non HTTPS dans un binaire de publication.
3. **Clé : celle du fichier `apps/desktop/src-tauri/update-key.pub`, embarquée à la compilation** (`include_str!`), jamais lue sur le disque du PC. Même schéma que l'agent : la clé du dépôt est une clé de DÉVELOPPEMENT dont la clé secrète n'existe pas (son commentaire porte « DEV public key ») ; tant que Voiky n'a pas mis la sienne, aucune signature n'est acceptée. Garde de publication : `cargo xtask client-release-check`, exécuté en premier par le flux de publication, refuse ce fichier. Les tests signent avec des paires jetées à chaque exécution (aucun secret écrit). Procédure : `docs/runbooks/publier-une-version-du-client.md`. La clé est passée au greffon par `Builder::pubkey` ; `tauri.conf.json` porte `plugins.updater.pubkey: ""` (la configuration du greffon l'exige, la valeur est remplacée). **Signature de publication par `cargo xtask client-sign`** (crate `minisign`, clés au format de `tauri signer generate`, vérifié sur une paire jetée) et non par `tauri signer sign` : le job de signature n'installe aucune dépendance npm (la clé secrète n'est donnée qu'à l'étape qui signe, après la construction, dans un job séparé ; voir le flux `publish-client`), et la version est inscrite dans le commentaire signé (`version:X.Y.Z`), ce que `tauri signer sign` ne fait qu'avec `--app-version`. `client-sign` relit sa signature contre `update-key.pub` : une clé secrète qui n'est pas la paire de la clé publique du dépôt est refusée avant toute publication.
4. **Sécurité de l'état de la mise à jour** : (a) rien n'est installé ni téléchargé sans clic (`begin_install` est le seul chemin, appelé par la seule commande `install_update`) ; (b) pas de rétrogradation : une version égale ou inférieure est ignorée (comparaison semver côté greffon ET côté domaine) ; préversions et métadonnées de construction refusées ; **`requireSignedVersion` est ACTIVÉ** (`tauri.conf.json`) : la version annoncée par le manifeste doit être celle du commentaire signé de l'installateur, donc un ancien installateur, validement signé, ne peut pas être servi sous un numéro plus grand (rejeu) ; une signature sans version est refusée (tests `an_old_installer_validly_signed…`, `a_signature_without_a_version_is_refused`) ; (c) signature vérifiée avant toute écriture (téléchargement en mémoire) ; (d) notes de version affichées en texte brut, jamais en HTML, bornées à 8 000 caractères ; (e) aucun appel réseau hors de la vérification prévue (une par jour au plus, plus « Vérifier maintenant ») et du clic.
5. **Stockage de la fréquence et du report : un fichier JSON côté Rust (`update.json` dans le dossier de données), PAS `localStorage`.** Le ticket disait `localStorage`, mais (i) la règle « jamais plus d'une fois par jour » doit tenir si la WebView est vidée, réinstallée ou si un utilisateur efface ses données de navigation ; (ii) la règle du dépôt interdit `localStorage` à l'interface (« le front ne persiste rien », ADR-0010) ; (iii) le port `UpdateStore` rend la règle testable avec une horloge injectée. Contenu : dernière tentative (réussie ou non), dernière réussite, fin du report, version annoncée et ses notes. Écriture atomique (fichier voisin puis renommage) ; fichier illisible = état par défaut (le client démarre toujours). La tentative est écrite AVANT l'appel réseau.
6. **Fréquence : au plus UNE requête de vérification automatique par 24 h, jamais plus ; une tentative qui n'a pas pu émettre de requête ne consomme pas le quota.** (a) la lettre de BR-UPDATE-001 est tenue : `last_request_at` (écrit AVANT l'appel réseau) borne les vérifications automatiques à une par 24 h. (b) Une vérification qui n'a PAS pu émettre de requête (connexion ou résolution du nom impossible : `FeedError::no_request_sent`) restaure `last_request_at` : elle sera retentée au battement horaire suivant, jusqu'à ce que le réseau soit là, toujours dans la limite du (a) puisqu'aucune requête n'est partie. (c) Un échec APRÈS l'émission (service muet, erreur 5xx, réponse invalide) consomme le quota : prochaine tentative le lendemain (BR-UPDATE-008), « Vérifier maintenant » restant disponible (au plus une fois par 30 s, pour qu'une page compromise ne boucle pas sur la requête). Pas de boucle de relance, pas de sondage continu : le battement est horaire et ne fait aucune requête tant qu'il n'y a rien à vérifier. Choix de l'agent principal, que Voiky peut changer ; ce qui le motive : un PC dont le réseau n'est pas prêt au démarrage de Windows ne devait pas rester sans vérification pendant un jour, ni indéfiniment si l'allumage est chaque jour un peu plus tard.
7. **Forme du code** : domaine pur (`update/domain.rs` : fréquence, report, validation d'annonce, horloge injectée), cas d'usage (`service.rs`), ports (`ports.rs` : `Clock`, `UpdateStore`, `Feed`, `StateSink`), adaptateurs (`store.rs` : fichier ; `feed.rs` : greffon), état complet publié à l'interface par l'événement `update://state` (`seq` croissant). Le bandeau est décidé côté Rust (`bannerVisible`) ; l'interface ne calcule aucune date.
8. **Le manifeste n'est lu que lorsqu'une version plus récente existe.** Le greffon ne rend `raw_json` que pour une annonce : client à jour = manifeste jeté. Une section `agent` du même `latest.json` n'est donc pas lisible quand le client est à jour, et `releases/latest` est unique (une release de l'agent seule, sans `latest.json` `windows-x86_64`, rend le client muet). **Point à reprendre par le lot interface de HRT-17** : soit une seconde requête (hors de la règle d'une par jour : à décider), soit un fichier à part (`agent.json`) dans la même release ; `client-manifest` réécrit le fichier entier, une section `agent` devra être fusionnée, pas perdue. Voir l'amendement d'ADR-0008.

## Dépendances et TLS

`tauri-plugin-updater` 2.13.1 tire `reqwest` 0.13 avec `rustls-no-provider`, `rustls` avec la seule fonction `ring`, `rustls-platform-verifier` (magasin de certificats de Windows), `minisign-verify` 0.2, `zip` 4, `semver`, `tar`, `infer`. **Ni `aws-lc-rs`/`aws-lc-sys`, ni OpenSSL, ni `native-tls`** (vérifié par `cargo tree -i aws-lc-rs`, `-i openssl`, `-i native-tls` : absents), donc la même pile TLS que `hearth-link` (ADR-0011) : pas de second fournisseur cryptographique dans le client. Le greffon s'appuie sur le fournisseur par défaut de `rustls`, qui s'installe tout seul quand une seule fonction (`ring`) est activée. Ajouts directs : `semver` (version), `reqwest` (politique de redirection, même version que le greffon), `url`, `base64` (clé), `async-trait` (port `Feed`) ; en dev : `minisign` (paires de test jetées). Le greffon ajoute aussi, pour d'autres plateformes, `objc2-osa-kit`, `core-foundation`, `tar`, `xattr` (inertes sous Windows).

## Comment l'appliquer

- Publier : `docs/runbooks/publier-une-version-du-client.md` (générer la paire, remplacer `update-key.pub`, créer deux secrets, lancer à la main le flux `publish-client`, publier le brouillon).
- Ajouter une commande : voir ADR-0010 (5 endroits). Toute commande de mise à jour doit continuer à ne prendre ni adresse ni chemin.
- Changer de clé : les clients installés n'acceptent que l'ancienne : réinstaller à la main une fois.

## Quand NE PAS l'appliquer / limites

- Le manifeste n'est PAS signé (seul l'installateur l'est) ; un manifeste forgé ne peut pas faire installer du code (signature), ni rejouer un ancien installateur sous un numéro plus grand (`requireSignedVersion`). Un installateur signé SANS version (signé par un autre outil) est refusé : toute release doit venir du flux `publish-client`.
- Le greffon télécharge en mémoire sans plafond de taille. Rien ne borne la taille côté client ; la signature n'est vérifiée qu'APRÈS le téléchargement complet. Ce qui limite l'exposition : source et redirections restreintes à GitHub en HTTPS, délai de 15 min, installateur de quelques dizaines de Mo. Un plafond demanderait de télécharger nous-mêmes (rejeté ici).
- Le dépôt public rend les notes de version et les installateurs publics (c'est voulu : dépôt AGPL).
- Windows 64 bits seulement (`windows-x86_64`).
- Non vérifié sans vraie publication : le lancement réel de l'installateur NSIS par le greffon, la relance du client, la conservation de l'entrée de démarrage de Windows après `/UPDATE` (runbook, § « Premier essai »).

## Alternatives rejetées

- **`localStorage` pour la fréquence et le report** : voir décision 5.
- **Mettre à jour par un code maison** (téléchargement + `minisign-verify` + lancement) : du code sensible de plus à maintenir pour le même résultat.
- **Accorder `updater:default` à la WebView et piloter en JavaScript** : ouvre les commandes du greffon (adresse, en-têtes, mandataire fournis par la page).
- **Flux configurable par l'utilisateur** : c'est précisément la porte qu'on ferme ; l'adresse est une constante.
- **Installer en silence dès la détection** : contraire à BR-UPDATE-002.

## Conséquences

- Un test d'intégration compile et exécute le greffon contre un serveur de versions local (`tests/update_feed.rs`, runtime Tauri simulé) : manifeste, signature valide, autre clé, fichier modifié, fichier plus court, coupure, serveur muet. Le lancement de l'installateur n'est jamais exécuté en test.
- `bindings.ts` gagne `UpdateStateDto` et ses types (régénération : `HEARTH_REGEN_BINDINGS=1 cargo test -p hearth-desktop`).
- Le flux `publish-client` ne se déclenche qu'à la main et ne crée qu'un brouillon.

## Références

- ADR-0008, ADR-0014, ADR-0010, ADR-0011 ; BR-UPDATE-001 à 010, 025, 026.
- tauri-plugin-updater : https://v2.tauri.app/plugin/updater/ · minisign : https://jedisct1.github.io/minisign/
