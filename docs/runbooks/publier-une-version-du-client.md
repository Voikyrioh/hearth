# Publier une version du client

Procédure d'exploitation de la mise à jour du client Windows (HRT-16). Règles : `BR-UPDATE-001` à `BR-UPDATE-010`, `025`, `026`. Décisions : ADR-0008, ADR-0017. Pour l'agent : `mettre-a-jour-agent.md` (même logique de clé, clé distincte). Ce runbook décrit le flux où le client lit ses mises à jour sur `hearth.voikyrioh.fr` (HRT-49, ADR-0035) et non plus sur GitHub.

## En bref

Le client vérifie au lancement puis une fois par jour au plus le manifeste servi par `https://hearth.voikyrioh.fr` (ADR-0035). Si une version plus récente existe, un bandeau le dit ; au clic sur « Mettre à jour maintenant », le client télécharge l'installateur en mémoire, vérifie sa signature minisign contre la clé publique **embarquée**, et seulement alors lance l'installateur (mode passif, relance). Pas de signature valide : rien n'est installé.

Publier = construire l'installateur, le signer avec TA clé secrète, puis le déposer dans le bucket derrière `https://hearth.voikyrioh.fr`. Le flux `publish-client` (déclenché à la main) construit et signe, et rend l'artefact `hearth-client-signed` : il ne dépose rien et ne crée AUCUNE release GitHub. Le dépôt dans le bucket se fait sur ta machine (`cargo xtask release-stage` puis `release-upload`, livrées par HRT-51, pas encore dans le dépôt), et c'est le manifeste du bucket, écrit en dernier, qui annonce la version aux clients. GitHub ne garde qu'un second canal : une release SANS binaire par version, créée à la main (voir plus bas).

## Prérequis (déjà en place)

Les clés de publication sont en place depuis le 2026-10-12 : la clé publique du dépôt (`apps/desktop/src-tauri/update-key.pub`) est celle de production du client (identifiant `70250B2A465F79C3`), et la clé secrète est dans l'environnement `release`. **Ne pas régénérer de paire et ne pas modifier `update-key.pub`** : un client installé n'accepterait plus aucune mise à jour. Le flux de publication refuse la clé de développement (`cargo xtask client-release-check`).

1. L'environnement `release` existe (Réglages > Environments, nom exact `release`) : *Required reviewers* (toi), *Deployment branches* limité à `main`. Le job `sign` le déclare ; sans lui, la garde « main seulement » du flux n'arrête qu'une erreur, pas quelqu'un qui a l'écriture sur le dépôt, et GitHub créerait un environnement SANS protection à la première exécution. Les deux secrets sont des secrets de cet ENVIRONNEMENT (pas du dépôt) : `HEARTH_CLIENT_SIGNING_KEY` et `HEARTH_CLIENT_SIGNING_KEY_PASSWORD` (mot de passe OBLIGATOIRE : la signature est refusée sans lui).
2. La clé secrète est sauvegardée ailleurs (gestionnaire de mots de passe) avec son mot de passe : sans elle, plus aucune mise à jour possible.

### En cas de perte ou de rotation de la clé

**Avertissement : changer la clé rend tous les clients installés incapables d'accepter la moindre mise à jour ; il faut les réinstaller un par un à la main.** À ne faire qu'en cas de perte avérée.

1. Générer la paire (la clé secrète reste chez toi, jamais dans le dépôt, jamais dans le hub) :
   ```powershell
   cd apps\desktop
   npx tauri signer generate -w "$env:USERPROFILE\.hearth\client-update.key"      # demande un mot de passe ; écrit la clé secrète et client-update.key.pub
   ```
2. Remplacer la clé publique du dépôt par la nouvelle (le fichier de Tauri, une ligne en base64, ou le fichier minisign : les deux formats sont acceptés), puis commit et PR comme d'habitude. Sous PowerShell : `Copy-Item $HOME\.hearth\client-update.key.pub apps\desktop\src-tauri\update-key.pub` ; en Bash : `cp ~/.hearth/client-update.key.pub apps/desktop/src-tauri/update-key.pub`.
3. Remplacer les secrets de l'environnement `release` :
   ```powershell
   Get-Content -Raw "$env:USERPROFILE\.hearth\client-update.key" | gh secret set HEARTH_CLIENT_SIGNING_KEY --env release
   gh secret set HEARTH_CLIENT_SIGNING_KEY_PASSWORD --env release
   ```
   Sous PowerShell, `<` n'existe pas : `Get-Content -Raw $HOME\.hearth\client-update.key | gh secret set HEARTH_CLIENT_SIGNING_KEY --env release`.

## Publier une version

1. Choisir la version `X.Y.Z` (pas de préversion). La changer à **deux** endroits, dans une PR : `version` de `[workspace.package]` du `Cargo.toml` racine et `version` de `apps/desktop/package.json`. C'est la SEULE source du numéro : le flux ne demande aucune version dans son formulaire, il lit celle du dépôt (`cargo xtask client-version`), donc le numéro signé est celui du binaire construit.
2. Fusionner la PR, puis : Actions > **publish-client** > Run workflow, **depuis `main`** (le flux refuse toute autre branche : le binaire signé doit venir de ce qui est fusionné), avec les notes de version (texte brut : elles s'affichent telles quelles dans le client, en une liste de lignes par exemple).
3. Le flux, en deux jobs : **`build`** (aucun secret) : garde de publication, construction de l'installateur NSIS (`Hearth_X.Y.Z_x64-setup.exe`), installateur passé en artefact. **Avant d'approuver `sign`, relis le journal de `build`** (voir plus bas : c'est la seule relecture qui retient encore quelque chose). **`sign`** (installe aucune dépendance npm, droit `contents: read` seulement) : compile `xtask`, puis une seule étape voit la clé secrète : `cargo xtask client-sign` signe avec la version dans le commentaire signé (`version:X.Y.Z`) et RELIT la signature contre `update-key.pub` (une clé secrète qui n'est pas la paire du fichier du dépôt est refusée ici) ; `client-manifest` revérifie, puis écrit `latest.json` avec l'adresse `https://hearth.voikyrioh.fr/api/releases/X.Y.Z/Hearth_X.Y.Z_x64-setup.exe` ; enfin l'artefact **`hearth-client-signed`** (conservé 1 jour) : `Hearth_X.Y.Z_x64-setup.exe`, `Hearth_X.Y.Z_x64-setup.exe.sig`, `latest.json`, `notes.txt`. Le journal affiche les sommes SHA-256 de ces fichiers. Aucune release n'est créée.
4. Sur ta machine (Windows PowerShell), télécharger l'artefact, puis le SUPPRIMER de GitHub (il est public tant qu'il existe) :
   ```sh
   gh run download <id-du-run> --repo Voikyrioh/hearth -n hearth-client-signed -D <dossier>
   gh api repos/Voikyrioh/hearth/actions/runs/<id-du-run>/artifacts
   gh api -X DELETE repos/Voikyrioh/hearth/actions/artifacts/<id-de-l-artefact>
   ```
   La deuxième commande liste les artefacts du run : prendre l'`id` de celui qui s'appelle `hearth-client-signed`. Compare les sommes SHA-256 du journal à celles des fichiers téléchargés (`Get-FileHash <fichier>`). Ensuite : construire et signer l'agent (`mettre-a-jour-agent.md`), puis assembler et déposer, depuis la racine du dépôt à jour sur `main` (commandes livrées par HRT-51 ; si `cargo xtask` ne les connaît pas, ce ticket n'est pas encore fusionné : s'arrêter là) :
   ```powershell
   cargo xtask release-stage --client-dir <dossier> --agent-binary <hearth-agent> --agent-signature <hearth-agent.minisig>
   cargo xtask release-upload
   cargo xtask release-upload --confirm
   ```
   `release-stage` revérifie tout (signatures, sommes, adresses, script) et écrit sous `target/release-stage/` seulement. `release-upload` sans option est un ESSAI À BLANC : il affiche chaque écriture et n'en fait aucune ; le relire avant `--confirm`. `--confirm` demande l'outil `aws`, `curl`, et la clé de publication du bucket dans `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` de la session (jamais en argument, jamais dans un fichier du dépôt). Il refuse de réécrire une version déjà déposée avec un autre contenu, et de remplacer un manifeste d'une version supérieure (`--allow-rollback` seulement pour retirer une version, voir plus bas). Le manifeste est écrit en dernier et annonce la version aux clients, au plus une vérification par jour et par client, ou leur « Vérifier maintenant ».
5. **Release GitHub sans binaire**, à la main, APRÈS le dépôt réussi (décision du propriétaire du 2026-10-12 ; second canal pour les sommes, aucun flux ne la crée). Les notes sont `notes.txt` du dossier de `gh run download` ; `SHA256SUMS` est le fichier préparé par `release-stage` (le même que celui du bucket : `target\release-stage\releases\X.Y.Z\SHA256SUMS`). Tant que cette étiquette n'est pas posée, rien ne refuse de relancer le flux sur le même numéro (`client-release-check` ne regarde que l'étiquette `vX.Y.Z` de `origin`) : ne pas relancer `publish-client` entre la signature et cette étape ; après le premier objet déposé, c'est `release-upload` qui refuse un contenu différent. Le SHA de l'étiquette est celui du commit sur lequel le flux a tourné :
   ```powershell
   gh run view <id-du-run> --repo Voikyrioh/hearth --json headSha --jq .headSha
   gh release create "vX.Y.Z" --repo Voikyrioh/hearth `
     --target <SHA-donné-par-la-commande-précédente> `
     --title "Hearth X.Y.Z" --notes-file <dossier>\notes.txt `
     target\release-stage\releases\X.Y.Z\SHA256SUMS
   ```
   En Bash, la continuation de ligne est `\` au lieu de la backtick. Aucun binaire n'est joint : ni installateur, ni `.sig`, ni `latest.json`. L'étiquette pointe la source du binaire (dépôt AGPL) ; une étiquette posée sur le mauvais commit ne se corrige pas proprement. Pas de `--draft` : la release ne pilote plus aucune mise à jour. Commande à lancer hors des heures employeur (gel Git du propriétaire).

Retirer une version publiée par erreur : remettre le manifeste de la version précédente dans le bucket (`releases/PREC/manifest.json` vers `releases/manifest.json`, conception distribution Scaleway, section 8.2), le numéro est brûlé ; les clients qui l'ont déjà installée ne reviennent pas en arrière (jamais de rétrogradation, ADR-0017) : publier une version suivante qui corrige. La release GitHub déjà créée se supprime ou s'annote à la main.

## Ce que la séparation en deux jobs protège, et ce qu'elle ne protège pas

- La clé secrète n'est vue que par l'étape `client-sign` du job `sign` (et elle exige l'approbation de l'environnement `release`). Les scripts de construction (`npm ci`, `build.rs` de tout l'arbre Cargo du client) tournent dans `build`, SANS secret.
- Les scripts de construction des dépendances de `xtask` (`cargo build -p xtask --locked`, arbre Cargo verrouillé et réduit) tournent dans `sign` juste AVANT l'étape qui reçoit la clé, mais sans la clé.
- **`sign` signe ce que `build` a produit.** Une dépendance compromise pendant `build` obtient donc UN installateur signé. Il est téléchargeable par TOUT compte GitHub connecté dès la fin du job `sign` (dépôt public), jusqu'à la suppression de l'artefact (1 jour au plus) : le numéro est brûlé dès que `sign` a réussi, pas au dépôt dans le bucket. La seule barrière est donc l'approbation de l'environnement `release` : la relecture qui compte est celle du **journal du job `build`** (dépendances installées, étapes, durée), **AVANT d'approuver**. Relire les fichiers de l'artefact après coup ne retient plus rien. Supprime l'artefact sitôt téléchargé (étape 4).
- L'artefact de `build` ne peut pas venir d'une autre exécution (portée de l'exécution).
- Le flux n'a aucun droit d'écriture sur le dépôt (`contents: read`) et ne crée ni brouillon, ni release, ni étiquette.

## Premier essai (à faire une fois, avant de compter dessus)

L'essai bout en bout (deux versions consécutives, dépôt dans le bucket) est porté par le ticket HRT-56 : ce runbook n'en décrit que les contrôles. Rien de ce qui suit n'a été vérifié sur une vraie version ; le faire avec deux versions consécutives :

1. La première publication est `0.1.0` PAR LE FLUX puis le dépôt dans le bucket (le client `0.1.0` des tests locaux n'a pas ta clé et ne peut pas être mis à jour). Installe cet installateur `0.1.0` sur un poste, puis publie `0.1.1` de la même façon.
2. Sur ce poste : « Vérifier maintenant » dans Réglages doit annoncer `0.1.1` et ses notes ; « Plus tard » masque le bandeau ; « Mettre à jour maintenant » télécharge, installe et relance le client en `0.1.1`.
3. Contrôler après la relance : les serveurs enregistrés et leurs mots de passe sont là, les réglages aussi, et **l'entrée de démarrage avec Windows** (clé `Run` de l'utilisateur) existe toujours si elle était activée (BR-UPDATE-005).
4. Contrôler `%APPDATA%\fr.voikyrioh.hearth\update.json` : la dernière vérification et la version qui tourne y sont ; le journal du client (`logs/`) contient « clé de mise à jour de développement » seulement pour un client construit avec la clé du dépôt.
5. Refus : signer une copie de l'installateur avec une AUTRE clé et la servir à la place (adresse de test, HRT-56), ou modifier un octet : le client doit répondre « Mise à jour corrompue. Refusée. » et rien ne doit s'installer.
6. Contrôler le `.sig` publié : décodé (base64), son commentaire de confiance porte `version:X.Y.Z` (`client-sign` l'impose, c'est vérifiable à l'œil).
7. **Sortie du client** (le greffon fait `exit(0)` juste après avoir lancé l'installateur, sans crochet de notre côté) : (a) la relance reprend les arguments du processus : un client lancé par Windows avec `--minimized` doit revenir caché dans la zone de notification après un clic sur « Mettre à jour maintenant » ; (b) `installer/hooks.nsh` : si le contrôle « disque insuffisant » refuse en mode passif, le client est fermé et non relancé (l'ancienne version reste installée) : noter ce que tu vois.
8. **Case de démarrage de l'installateur** (HRT-21, ADR-0026), sur un poste de test. La CI en rejoue l'essentiel sur son runner Windows jetable (`apps/desktop/scripts/installer-ci.ps1`, étape du job `desktop`, artefact `hearth-installer-evidence` : captures de la page d'accueil et de la page de fin, états de la case, `/S`, réinstallation, `/UPDATE /P`, parcours complet avec un vrai clic sur « Fermer », désinstallation, dossier d'installation avec une espace, valeur exacte entre guillemets, une seule valeur `Hearth`, lancement de la valeur par `CreateProcess`) ; il reste à faire à la main, avant la première publication : Regarder à chaque fois la valeur `Hearth` de `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` et le réglage « Lancer Hearth au démarrage de Windows » de l'application : (a) installation à l'écran, case laissée décochée : valeur absente, réglage désactivé ; (b) case cochée : valeur `"…\hearth-desktop.exe" --minimized` (chemin ENTRE GUILLEMETS, HRT-29) présente, réglage activé au premier lancement, la fenêtre reste cachée au prochain démarrage de Windows ; (c) `Hearth_X.Y.Z_x64-setup.exe /S` sur un poste neuf : aucune valeur ; (d) activer le réglage dans l'application, puis « Mettre à jour maintenant » (mise à jour passive) : la valeur est toujours là ; désactiver le réglage puis mettre à jour : la valeur reste absente ; (e) lancer l'installateur à la main par-dessus une installation qui a l'entrée : la case est cochée d'emblée ; la décocher retire la valeur ; (f) désinstaller : la valeur est retirée ; (h) **« Fermer » ferme l'installateur** à la dernière page, avec la souris, la case « Créer un raccourci » cochée puis décochée (la CI le fait par un vrai clic de souris sur son runner) ; (i) **échelles d'affichage 125 % et 150 %** : la page d'accueil, la case et l'aide restent entières, rien ne se chevauche (la CI ne couvre que 100 %) ; (j) **chemin d'installation avec une espace** (CONTRÔLE de l'ouverture de session réelle, à faire avant la première publication ; la valeur entre guillemets est déjà écrite par l'application et l'installateur, HRT-29, et n'attend plus ce résultat ; noter ici le résultat et la version de Windows, et aussi : une valeur ancienne sans guillemets, posée à la main, est réécrite entre guillemets au premier lancement, sans réactiver une entrée désactivée dans le Gestionnaire des tâches) (profil Windows « Jean Dupont » : `C:\Users\Jean Dupont\AppData\Local\Hearth`) : activer le démarrage, ouvrir une session Windows, Hearth démarre réduit dans la zone de notification ; (g) désactiver l'entrée de Hearth dans le Gestionnaire des tâches (onglet Démarrage), relancer l'installateur à la main : la case arrive DÉCOCHÉE, et un simple Suivant ne réactive rien. Depuis HRT-21, les cas (a) à (g) sauf l'échelle d'affichage sont rejoués sur le runner Windows jetable de la CI (`apps/desktop/scripts/installer-ci.ps1`, artefact `hearth-installer-evidence`) ; à la main il reste surtout le visuel à 125 et 150 % d'échelle. Vérifier aussi le visuel de la page d'accueil (case et aide entières, rien ne se chevauche) à 100 % et à 150 % d'échelle.

## Si ça ne marche pas

| Constat | Cause probable | Que faire |
|---|---|---|
| Le flux s'arrête à « Garde de publication » | la version demandée n'est pas celle de `Cargo.toml`/`package.json`, ou l'étiquette `vX.Y.Z` existe déjà sur `origin` (numéro déjà publié : en prendre un autre), ou `update-key.pub` est redevenue la clé de développement (ne doit plus arriver depuis la pose de la clé de publication) | suivre le message |
| `client-sign` : « HEARTH_CLIENT_SIGNING_KEY est absent ou vide », « mot de passe obligatoire », « clé secrète illisible » | secret absent (ou posé dans le dépôt et non dans l'environnement `release`), mot de passe vide ou faux | créer ou corriger les secrets de l'environnement (prérequis 1) |
| Le job `sign` attend | l'approbation de l'environnement `release` | l'approuver, après relecture du journal de `build` |
| `client-sign` ou `client-manifest` : « la signature ne correspond pas à update-key.pub » | la clé secrète des secrets n'est pas la paire de la clé publique du dépôt | remettre la bonne clé secrète dans l'environnement `release` (sauvegarde, prérequis 2) ; ne PAS remplacer `update-key.pub` |
| Le flux ne démarre pas / ne fait rien | lancé depuis une autre branche que `main` | Run workflow depuis `main` |
| Le client ne voit jamais la mise à jour | manifeste du bucket pas encore écrit (dépôt interrompu avant l'étape 4 de `release-upload`) ; ou sans l'entrée `windows-x86_64` ; ou dernière vérification il y a moins de 24 h | relancer `release-upload` ; « Vérifier maintenant » |
| « Mise à jour corrompue. Refusée. » alors que le fichier est bon | le client a été construit avec une autre clé publique que celle qui a signé (clé de développement du dépôt, ou ancienne clé) | réinstaller à la main un client construit avec la bonne clé |
| « Téléchargement interrompu. » | coupure, ou hearth.voikyrioh.fr répond en erreur ; ou l'adresse du manifeste pointe un fichier absent du bucket | « Réessayer » ; vérifier que les fichiers de la version sont dans le bucket (`GET /api/releases/X.Y.Z/...`) |
| Aucun message d'erreur et pas de bandeau | normal sans Internet ou si hearth.voikyrioh.fr ne répond pas (BR-UPDATE-007, 008) : la date de la dernière vérification reste dans Réglages | « Vérifier maintenant » plus tard |

## Limites connues

- Le manifeste n'est pas signé (HTTPS vers hearth.voikyrioh.fr) ; seul l'installateur l'est. Un manifeste forgé ne peut pas rejouer un ancien installateur sous un numéro plus grand : le client exige que la version annoncée soit celle du commentaire signé (`requireSignedVersion`, activé). Toute version doit donc venir du flux `publish-client` ; un installateur signé par un autre outil, sans version, est refusé par les clients.
- Au plus une requête de vérification automatique par 24 h, et au plus 3 tentatives réseau automatiques par 24 h glissantes quoi qu'il arrive. Seul un échec de résolution du nom ou de connexion TCP au PREMIER hôte rend la journée (retentée au battement horaire, dans la limite des 3) ; échec TLS, échec à un saut suivant, délai dépassé, réponse invalide la consomment : prochaine tentative le lendemain, ou « Vérifier maintenant » (au plus une fois par 30 s).
- Redirections : HTTPS à chaque saut, au plus 5 (le serveur de hearth.voikyrioh.fr n'en fait aucune, ADR-0035), AUCUNE liste d'hôtes sur les sauts suivants (ADR-0017 : la signature protège le contenu, HTTPS le transport).
- Le manifeste unique est `releases/manifest.json` du bucket, servi tel quel par `https://hearth.voikyrioh.fr/api/releases/latest` (ADR-0035) : il porte l'entrée `windows-x86_64` du client et la section `agent` (ADR-0021), assemblées par `release-stage` (HRT-51), qui est le seul à l'écrire. `publish-client` ne fabrique ni ne signe la section `agent` (runbook `mettre-a-jour-agent.md`) ; le `latest.json` de son artefact n'est qu'un intermédiaire, jamais déposé tel quel. Le greffon est lu avec un comparateur de versions « toujours vrai » (une version du client égale ou inférieure reste ignorée par le client lui-même) : une vérification reste UNE requête. Une release GitHub n'a aucun effet sur les clients.
- Windows 64 bits seulement.

## Vérifier de bout en bout (développement)

`cargo test -p hearth-desktop --test update_feed` : le greffon contre un serveur de versions local, avec des paires de clés jetées (manifeste, signature valide, autre clé, fichier modifié ou plus court, coupure, serveur muet). `cargo test -p xtask` : manifeste et garde de publication. L'installation réelle de l'installateur n'est jamais lancée en test : voir « Premier essai ».
