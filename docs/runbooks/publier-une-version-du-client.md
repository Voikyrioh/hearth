# Publier une version du client

Procédure d'exploitation de la mise à jour du client Windows (HRT-16). Règles : `BR-UPDATE-001` à `BR-UPDATE-010`, `025`, `026`. Décisions : ADR-0008, ADR-0017. Pour l'agent : `mettre-a-jour-agent.md` (même logique de clé, clé distincte).

## En bref

Le client vérifie au lancement puis une fois par jour au plus le fichier `latest.json` de la **dernière release publiée** de `Voikyrioh/hearth`. Si une version plus récente existe, un bandeau le dit ; au clic sur « Mettre à jour maintenant », le client télécharge l'installateur en mémoire, vérifie sa signature minisign contre la clé publique **embarquée**, et seulement alors lance l'installateur (mode passif, relance). Pas de signature valide : rien n'est installé.

Publier = construire l'installateur, le signer avec TA clé secrète, joindre `latest.json`, et publier la release. Tout est fait par le flux `publish-client` (déclenché à la main), qui crée un **brouillon** : tant que tu ne l'as pas publié, aucun client ne le voit.

## Prérequis, une fois : la paire de clés

La clé publique du dépôt (`apps/desktop/src-tauri/update-key.pub`) est une clé de **développement** : aucune clé secrète n'existe, donc **aucune mise à jour n'est possible** tant que tu n'as pas mis la tienne. Le flux de publication refuse cette clé (`cargo xtask client-release-check`).

1. Générer la paire (la clé secrète reste chez toi, jamais dans le dépôt, jamais dans le hub) :
   ```sh
   cd apps/desktop
   npx tauri signer generate -w ~/.hearth/client-update.key      # demande un mot de passe ; écrit la clé secrète et client-update.key.pub
   ```
2. Remplacer la clé publique du dépôt par celle-ci (le fichier de Tauri, une ligne en base64, ou le fichier minisign : les deux formats sont acceptés) :
   ```sh
   cp ~/.hearth/client-update.key.pub apps/desktop/src-tauri/update-key.pub
   ```
   Puis commit et PR comme d'habitude. Un client construit AVANT ce remplacement n'acceptera jamais une release signée par ta clé : il faut le réinstaller une fois à la main (même règle que pour l'agent). Changer de clé plus tard = réinstaller à la main tous les clients.
3. **Créer l'environnement `release`** (Réglages > Environments > New environment, nom exact `release`), AVANT la première exécution : *Required reviewers* (toi), *Deployment branches* limité à `main`. Le job `sign` le déclare ; sans lui, la garde « main seulement » du flux n'arrête qu'une erreur, pas quelqu'un qui a l'écriture sur le dépôt (il peut lancer le flux depuis sa branche, garde retirée comprise), et GitHub créerait un environnement SANS protection à la première exécution. Puis créer les deux secrets comme secrets de cet ENVIRONNEMENT (pas du dépôt) :
   ```sh
   gh secret set HEARTH_CLIENT_SIGNING_KEY --env release < ~/.hearth/client-update.key
   gh secret set HEARTH_CLIENT_SIGNING_KEY_PASSWORD --env release         # le mot de passe de la clé : OBLIGATOIRE (la signature est refusée sans mot de passe ; choisis-en un à la génération)
   ```
4. Sauvegarder `~/.hearth/client-update.key` et son mot de passe ailleurs (gestionnaire de mots de passe) : sans elle, plus aucune mise à jour possible.

## Publier une version

1. Choisir la version `X.Y.Z` (pas de préversion). La changer à **deux** endroits, dans une PR : `version` de `[workspace.package]` du `Cargo.toml` racine et `version` de `apps/desktop/package.json`. C'est la SEULE source du numéro : le flux ne demande aucune version dans son formulaire, il lit celle du dépôt (`cargo xtask client-version`), donc le numéro signé est celui du binaire construit.
2. Fusionner la PR, puis : Actions > **publish-client** > Run workflow, **depuis `main`** (le flux refuse toute autre branche : le binaire signé doit venir de ce qui est fusionné), avec les notes de version (texte brut : elles s'affichent telles quelles dans le client, en une liste de lignes par exemple).
3. Le flux, en deux jobs : **`build`** (aucun secret) : garde de publication, construction de l'installateur NSIS (`Hearth_X.Y.Z_x64-setup.exe`), installateur passé en artefact. **`sign`** (installe aucune dépendance npm) : compile `xtask`, puis une seule étape voit la clé secrète : `cargo xtask client-sign` signe avec la version dans le commentaire signé (`version:X.Y.Z`) et RELIT la signature contre `update-key.pub` (une clé secrète qui n'est pas la paire du fichier du dépôt est refusée ici, avant toute publication) ; `client-manifest` revérifie, puis écrit `latest.json` ; enfin le **brouillon** de release `vX.Y.Z`, étiqueté sur le commit construit (`--target`), avec les trois fichiers (installateur, `.sig`, `latest.json`).
4. Ouvrir le brouillon, vérifier les trois fichiers et les notes, puis **Publish release**. À partir de là, `releases/latest/download/latest.json` annonce la version aux clients (au plus une vérification par jour et par client, ou leur « Vérifier maintenant »).

Retirer une version publiée par erreur : repasser la release en brouillon ou la supprimer ; les clients qui l'ont déjà installée ne reviennent pas en arrière (jamais de rétrogradation, ADR-0017) : publier une version suivante qui corrige.

## Ce que la séparation en deux jobs protège, et ce qu'elle ne protège pas

- La clé secrète n'est vue que par l'étape `client-sign` du job `sign` (et elle exige l'approbation de l'environnement `release`). Les scripts de construction (`npm ci`, `build.rs` de tout l'arbre Cargo du client) tournent dans `build`, SANS secret.
- Les scripts de construction des dépendances de `xtask` (`cargo build -p xtask --locked`, arbre Cargo verrouillé et réduit) tournent dans `sign` juste AVANT l'étape qui reçoit la clé, mais sans la clé.
- **`sign` signe ce que `build` a produit.** Une dépendance compromise pendant `build` obtient donc UNE release signée (en brouillon). Avant d'approuver l'environnement, **relis le journal du job `build`** (dépendances installées, étapes, durée) ; avant de publier le brouillon, relis ses fichiers.
- L'artefact de `build` ne peut pas venir d'une autre exécution (portée de l'exécution).

## Premier essai (à faire une fois, avant de compter dessus)

Rien de ce qui suit n'a pu être vérifié sans vraie release ; le faire avec deux versions consécutives :

1. La première publication est `0.1.0` PAR LE FLUX (le client `0.1.0` des tests locaux n'a pas ta clé et ne peut pas être mis à jour). Installe cet installateur `0.1.0` sur un poste, puis publie `0.1.1` par le flux.
2. Sur ce poste : « Vérifier maintenant » dans Réglages doit annoncer `0.1.1` et ses notes ; « Plus tard » masque le bandeau ; « Mettre à jour maintenant » télécharge, installe et relance le client en `0.1.1`.
3. Contrôler après la relance : les serveurs enregistrés et leurs mots de passe sont là, les réglages aussi, et **l'entrée de démarrage avec Windows** (clé `Run` de l'utilisateur) existe toujours si elle était activée (BR-UPDATE-005).
4. Contrôler `%APPDATA%\fr.voikyrioh.hearth\update.json` : la dernière vérification et la version qui tourne y sont ; le journal du client (`logs/`) contient « clé de mise à jour de développement » seulement pour un client construit avec la clé du dépôt.
5. Refus : signer une copie de l'installateur avec une AUTRE clé, la poser dans une release brouillon de test, ou modifier un octet : le client doit répondre « Mise à jour corrompue. Refusée. » et rien ne doit s'installer.
6. Contrôler le `.sig` publié : décodé (base64), son commentaire de confiance porte `version:X.Y.Z` (`client-sign` l'impose, c'est vérifiable à l'œil).
7. **Sortie du client** (le greffon fait `exit(0)` juste après avoir lancé l'installateur, sans crochet de notre côté) : (a) la relance reprend les arguments du processus : un client lancé par Windows avec `--minimized` doit revenir caché dans la zone de notification après un clic sur « Mettre à jour maintenant » ; (b) `installer/hooks.nsh` : si le contrôle « disque insuffisant » refuse en mode passif, le client est fermé et non relancé (l'ancienne version reste installée) : noter ce que tu vois.
8. **Case de démarrage de l'installateur** (HRT-21, ADR-0026), sur un poste de test (jamais fait en test automatique : l'installateur écrit dans le registre). Regarder à chaque fois la valeur `Hearth` de `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` et le réglage « Lancer Hearth au démarrage de Windows » de l'application : (a) installation à l'écran, case laissée décochée : valeur absente, réglage désactivé ; (b) case cochée : valeur `"…\Hearth.exe" --minimized` présente, réglage activé au premier lancement, la fenêtre reste cachée au prochain démarrage de Windows ; (c) `Hearth_X.Y.Z_x64-setup.exe /S` sur un poste neuf : aucune valeur ; (d) activer le réglage dans l'application, puis « Mettre à jour maintenant » (mise à jour passive) : la valeur est toujours là ; désactiver le réglage puis mettre à jour : la valeur reste absente ; (e) lancer l'installateur à la main par-dessus une installation qui a l'entrée : la case est cochée d'emblée ; la décocher retire la valeur ; (f) désinstaller : la valeur est retirée. Vérifier aussi le visuel de la page d'accueil (case et aide entières, rien ne se chevauche) à 100 % et à 150 % d'échelle.

## Si ça ne marche pas

| Constat | Cause probable | Que faire |
|---|---|---|
| Le flux s'arrête à « Garde de publication » | `update-key.pub` est la clé de développement, ou la version demandée n'est pas celle de `Cargo.toml`/`package.json` | suivre le message |
| `client-sign` : « HEARTH_CLIENT_SIGNING_KEY est absent ou vide », « mot de passe obligatoire », « clé secrète illisible » | secret absent (ou posé dans le dépôt et non dans l'environnement `release`), mot de passe vide ou faux | créer ou corriger les secrets de l'environnement (prérequis 3) |
| Le job `sign` attend | l'approbation de l'environnement `release` | l'approuver, après relecture du journal de `build` |
| `client-sign` ou `client-manifest` : « la signature ne correspond pas à update-key.pub » | la clé secrète des secrets n'est pas la paire de la clé publique du dépôt | remettre la bonne clé publique (prérequis 2) ou la bonne clé secrète |
| Le flux ne démarre pas / ne fait rien | lancé depuis une autre branche que `main` | Run workflow depuis `main` |
| Le client ne voit jamais la mise à jour | release encore en brouillon ; ou publiée comme préversion ; ou `latest.json` sans l'entrée `windows-x86_64` ; ou dernière vérification il y a moins de 24 h | publier la release (hors préversion) ; « Vérifier maintenant » |
| « Mise à jour corrompue. Refusée. » alors que le fichier est bon | le client a été construit avec une autre clé publique que celle qui a signé (clé de développement du dépôt, ou ancienne clé) | réinstaller à la main un client construit avec la bonne clé |
| « Téléchargement interrompu. » | coupure, ou GitHub répond en erreur ; ou l'adresse de `latest.json` pointe un fichier absent de la release | « Réessayer » ; vérifier que les trois fichiers sont joints au brouillon publié |
| Aucun message d'erreur et pas de bandeau | normal sans Internet ou si GitHub ne répond pas (BR-UPDATE-007, 008) : la date de la dernière vérification reste dans Réglages | « Vérifier maintenant » plus tard |

## Limites connues

- Le manifeste `latest.json` n'est pas signé (HTTPS vers GitHub) ; seul l'installateur l'est. Un manifeste forgé ne peut pas rejouer un ancien installateur sous un numéro plus grand : le client exige que la version annoncée soit celle du commentaire signé (`requireSignedVersion`, activé). Toute release doit donc venir du flux `publish-client` ; un installateur signé par un autre outil, sans version, est refusé par les clients.
- Au plus une requête de vérification automatique par 24 h, et au plus 3 tentatives réseau automatiques par 24 h glissantes quoi qu'il arrive. Seul un échec de résolution du nom ou de connexion TCP au PREMIER hôte rend la journée (retentée au battement horaire, dans la limite des 3) ; échec TLS, échec à un saut suivant, délai dépassé, réponse invalide la consomment : prochaine tentative le lendemain, ou « Vérifier maintenant » (au plus une fois par 30 s).
- Redirections : HTTPS à chaque saut, au plus 5 (GitHub en fait 2 aujourd'hui), AUCUNE liste d'hôtes sur les sauts suivants (ADR-0017 : la signature protège le contenu, HTTPS le transport).
- Le `latest.json` de `releases/latest` est unique : ne publie jamais comme « dernière release » une release qui n'en porte pas avec l'entrée `windows-x86_64` (une release de l'agent seule rendrait le client muet) ; la cible de l'agent est une section `agent` du MÊME `latest.json` (ADR-0021), ajoutée à la main par `cargo xtask agent-manifest` (runbook `mettre-a-jour-agent.md`, « Publier la cible pour le client ») : `publish-client` ne la fabrique ni ne la signe, et refaire `client-manifest` écraserait le fichier et la perdrait. Le greffon est lu avec un comparateur de versions « toujours vrai » (une version du client égale ou inférieure reste ignorée par le client lui-même) : une vérification reste UNE requête.
- Windows 64 bits seulement.

## Vérifier de bout en bout (développement)

`cargo test -p hearth-desktop --test update_feed` : le greffon contre un serveur de versions local, avec des paires de clés jetées (manifeste, signature valide, autre clé, fichier modifié ou plus court, coupure, serveur muet). `cargo test -p xtask` : manifeste et garde de publication. L'installation réelle de l'installateur n'est jamais lancée en test : voir « Premier essai ».
