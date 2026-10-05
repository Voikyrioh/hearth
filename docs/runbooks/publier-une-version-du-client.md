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
3. Créer les deux secrets du dépôt (Réglages > Secrets and variables > Actions, ou en ligne de commande) :
   ```sh
   gh secret set HEARTH_CLIENT_SIGNING_KEY < ~/.hearth/client-update.key
   gh secret set HEARTH_CLIENT_SIGNING_KEY_PASSWORD         # le mot de passe de la clé (vide si aucune)
   ```
4. Sauvegarder `~/.hearth/client-update.key` et son mot de passe ailleurs (gestionnaire de mots de passe) : sans elle, plus aucune mise à jour possible.

## Publier une version

1. Choisir la version `X.Y.Z` (pas de préversion). La changer à **deux** endroits, dans une PR : `version` de `[workspace.package]` du `Cargo.toml` racine et `version` de `apps/desktop/package.json`. Le flux refuse une version qui n'est pas celle du dépôt.
2. Fusionner la PR, puis : Actions > **publish-client** > Run workflow, avec la version et les notes de version (texte brut : elles s'affichent telles quelles dans le client, en une liste de lignes par exemple).
3. Le flux : garde de publication, construction de l'installateur NSIS (`Hearth_X.Y.Z_x64-setup.exe`), signature (`tauri signer sign`, secrets ci-dessus), fabrication de `latest.json` (`cargo xtask client-manifest`), création du **brouillon** de release `vX.Y.Z` avec les trois fichiers (installateur, `.sig`, `latest.json`).
4. Ouvrir le brouillon, vérifier les trois fichiers et les notes, puis **Publish release**. À partir de là, `releases/latest/download/latest.json` annonce la version aux clients (au plus une vérification par jour et par client, ou leur « Vérifier maintenant »).

Retirer une version publiée par erreur : repasser la release en brouillon ou la supprimer ; les clients qui l'ont déjà installée ne reviennent pas en arrière (jamais de rétrogradation, ADR-0017) : publier une version suivante qui corrige.

## Premier essai (à faire une fois, avant de compter dessus)

Rien de ce qui suit n'a pu être vérifié sans vraie release ; le faire avec deux versions consécutives :

1. Publier `0.1.1` après avoir remplacé la clé (client `0.1.0` construit AVEC ta clé installé sur un poste).
2. Sur ce poste : « Vérifier maintenant » dans Réglages doit annoncer `0.1.1` et ses notes ; « Plus tard » masque le bandeau ; « Mettre à jour maintenant » télécharge, installe et relance le client en `0.1.1`.
3. Contrôler après la relance : les serveurs enregistrés et leurs mots de passe sont là, les réglages aussi, et **l'entrée de démarrage avec Windows** (clé `Run` de l'utilisateur) existe toujours si elle était activée (BR-UPDATE-005).
4. Contrôler `%APPDATA%\fr.voikyrioh.hearth\update.json` : la dernière vérification et la version qui tourne y sont ; le journal du client (`logs/`) contient « clé de mise à jour de développement » seulement pour un client construit avec la clé du dépôt.
5. Refus : signer une copie de l'installateur avec une AUTRE clé, la poser dans une release brouillon de test, ou modifier un octet : le client doit répondre « Mise à jour corrompue. Refusée. » et rien ne doit s'installer.

## Si ça ne marche pas

| Constat | Cause probable | Que faire |
|---|---|---|
| Le flux s'arrête à « Garde de publication » | `update-key.pub` est la clé de développement, ou la version demandée n'est pas celle de `Cargo.toml`/`package.json` | suivre le message |
| « le secret HEARTH_CLIENT_SIGNING_KEY n'existe pas » | secret absent | créer les secrets (prérequis 3) |
| Le client ne voit jamais la mise à jour | release encore en brouillon ; ou publiée comme préversion ; ou `latest.json` sans l'entrée `windows-x86_64` ; ou dernière vérification il y a moins de 24 h | publier la release (hors préversion) ; « Vérifier maintenant » |
| « Mise à jour corrompue. Refusée. » alors que le fichier est bon | le client a été construit avec une autre clé publique que celle qui a signé (clé de développement du dépôt, ou ancienne clé) | réinstaller à la main un client construit avec la bonne clé |
| « Téléchargement interrompu. » | coupure, ou GitHub répond en erreur ; ou l'adresse de `latest.json` pointe un fichier absent de la release | « Réessayer » ; vérifier que les trois fichiers sont joints au brouillon publié |
| Aucun message d'erreur et pas de bandeau | normal sans Internet ou si GitHub ne répond pas (BR-UPDATE-007, 008) : la date de la dernière vérification reste dans Réglages | « Vérifier maintenant » plus tard |

## Limites connues

- Le manifeste `latest.json` n'est pas signé (HTTPS vers GitHub) ; seul l'installateur l'est. La source de l'installateur est restreinte au dépôt et la version annoncée doit être plus récente. L'option `requireSignedVersion` du greffon (version annoncée = version signée) est désactivée : à activer dans `tauri.conf.json` (`plugins.updater.requireSignedVersion: true`) après avoir vérifié que la signature produite par `tauri signer sign` porte `version:X.Y.Z` dans son commentaire de confiance (`minisign -V` ou ouvrir le `.sig` décodé).
- Une vérification qui échoue (PC sans réseau au démarrage de Windows) compte pour la journée : la suivante est le lendemain, ou « Vérifier maintenant ».
- Windows 64 bits seulement.

## Vérifier de bout en bout (développement)

`cargo test -p hearth-desktop --test update_feed` : le greffon contre un serveur de versions local, avec des paires de clés jetées (manifeste, signature valide, autre clé, fichier modifié ou plus court, coupure, serveur muet). `cargo test -p xtask` : manifeste et garde de publication. L'installation réelle de l'installateur n'est jamais lancée en test : voir « Premier essai ».
