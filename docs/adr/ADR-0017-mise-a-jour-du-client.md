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
2. **Flux : `https://github.com/Voikyrioh/hearth/releases/latest/download/latest.json`**, constante de la compilation (`update/domain.rs::FEED_URL`), pas un réglage. `latest` pointe la dernière release PUBLIÉE (ni brouillon ni préversion). Le manifeste est celui du greffon (`version`, `notes`, `pub_date`, `platforms.windows-x86_64.{signature,url}`), fabriqué par `cargo xtask client-manifest` ; il pourra recevoir plus tard une section `agent` (ADR-0008) sans gêner le greffon, qui ignore les champs inconnus. Les installateurs ne se téléchargent que depuis `github.com/Voikyrioh/hearth/releases/download/…` en HTTPS (`DownloadPolicy`), contrôlé AVANT d'afficher le bandeau et de retenir l'annonce ; le greffon refuse en plus tout flux non HTTPS dans un binaire de publication.
3. **Clé : celle du fichier `apps/desktop/src-tauri/update-key.pub`, embarquée à la compilation** (`include_str!`), jamais lue sur le disque du PC. Même schéma que l'agent : la clé du dépôt est une clé de DÉVELOPPEMENT dont la clé secrète n'existe pas (son commentaire porte « DEV public key ») ; tant que Voiky n'a pas mis la sienne, aucune signature n'est acceptée. Garde de publication : `cargo xtask client-release-check`, exécuté en premier par le flux de publication, refuse ce fichier. Les tests signent avec des paires jetées à chaque exécution (aucun secret écrit). Procédure : `docs/runbooks/publier-une-version-du-client.md`. La clé est passée au greffon par `Builder::pubkey` ; `tauri.conf.json` porte `plugins.updater.pubkey: ""` (la configuration du greffon l'exige, la valeur est remplacée).
4. **Sécurité de l'état de la mise à jour** : (a) rien n'est installé ni téléchargé sans clic (`begin_install` est le seul chemin, appelé par la seule commande `install_update`) ; (b) pas de rétrogradation : une version égale ou inférieure est ignorée (comparaison semver côté greffon ET côté domaine) ; préversions refusées ; (c) signature vérifiée avant toute écriture (téléchargement en mémoire) ; (d) notes de version affichées en texte brut, jamais en HTML, bornées à 8 000 caractères ; (e) aucun appel réseau hors de la vérification prévue (une par jour au plus, plus « Vérifier maintenant ») et du clic.
5. **Stockage de la fréquence et du report : un fichier JSON côté Rust (`update.json` dans le dossier de données), PAS `localStorage`.** Le ticket disait `localStorage`, mais (i) la règle « jamais plus d'une fois par jour » doit tenir si la WebView est vidée, réinstallée ou si un utilisateur efface ses données de navigation ; (ii) la règle du dépôt interdit `localStorage` à l'interface (« le front ne persiste rien », ADR-0010) ; (iii) le port `UpdateStore` rend la règle testable avec une horloge injectée. Contenu : dernière tentative (réussie ou non), dernière réussite, fin du report, version annoncée et ses notes. Écriture atomique (fichier voisin puis renommage) ; fichier illisible = état par défaut (le client démarre toujours). La tentative est écrite AVANT l'appel réseau.
6. **Fréquence : une tentative qui échoue compte.** « Jamais plus souvent » est lu strictement : au lancement, puis 24 h après la dernière tentative, réussie ou non. Conséquence : un PC dont le réseau n'est pas encore prêt au démarrage de Windows ne revérifie qu'au lendemain (le client reste ouvert dans la zone de notification, un battement horaire sans réseau décide) ou à un « Vérifier maintenant ». Alternative écartée : retenter après un échec réseau (plus de requêtes, contraire au texte).
7. **Forme du code** : domaine pur (`update/domain.rs` : fréquence, report, validation d'annonce, horloge injectée), cas d'usage (`service.rs`), ports (`ports.rs` : `Clock`, `UpdateStore`, `Feed`, `StateSink`), adaptateurs (`store.rs` : fichier ; `feed.rs` : greffon), état complet publié à l'interface par l'événement `update://state` (`seq` croissant). Le bandeau est décidé côté Rust (`bannerVisible`) ; l'interface ne calcule aucune date.

## Dépendances et TLS

`tauri-plugin-updater` 2.13.1 tire `reqwest` 0.13 avec `rustls-no-provider`, `rustls` avec la seule fonction `ring`, `rustls-platform-verifier` (magasin de certificats de Windows), `minisign-verify` 0.2, `zip` 4, `semver`, `tar`, `infer`. **Ni `aws-lc-rs`/`aws-lc-sys`, ni OpenSSL, ni `native-tls`** (vérifié par `cargo tree -i aws-lc-rs`, `-i openssl`, `-i native-tls` : absents), donc la même pile TLS que `hearth-link` (ADR-0011) : pas de second fournisseur cryptographique dans le client. Le greffon s'appuie sur le fournisseur par défaut de `rustls`, qui s'installe tout seul quand une seule fonction (`ring`) est activée. Ajouts directs : `semver` (version), `url`, `base64` (clé), `async-trait` (port `Feed`) ; en dev : `minisign` (paires de test jetées). Le greffon ajoute aussi, pour d'autres plateformes, `objc2-osa-kit`, `core-foundation`, `tar`, `xattr` (inertes sous Windows).

## Comment l'appliquer

- Publier : `docs/runbooks/publier-une-version-du-client.md` (générer la paire, remplacer `update-key.pub`, créer deux secrets, lancer à la main le flux `publish-client`, publier le brouillon).
- Ajouter une commande : voir ADR-0010 (5 endroits). Toute commande de mise à jour doit continuer à ne prendre ni adresse ni chemin.
- Changer de clé : les clients installés n'acceptent que l'ancienne : réinstaller à la main une fois.

## Quand NE PAS l'appliquer / limites

- Le manifeste n'est PAS signé (seul l'installateur l'est). Un manifeste forgé ne peut pas faire installer du code (signature), mais pourrait annoncer une vieille version signée plus récente qu'elle n'est : barrières actuelles = HTTPS vers GitHub, source restreinte au dépôt, comparaison de versions. Barrière supplémentaire disponible : `requireSignedVersion` du greffon (la version annoncée doit être celle du commentaire signé). Désactivée ici, car la signature de publication (`tauri signer sign`) n'enregistre peut-être pas la version dans le commentaire de confiance ; à activer (tauri.conf.json) après avoir vérifié qu'une signature de publication le porte.
- Le greffon télécharge en mémoire sans plafond de taille ; la restriction de la source au dépôt public en tient lieu.
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
