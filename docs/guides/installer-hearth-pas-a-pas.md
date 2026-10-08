# Installer Hearth pas à pas

Ce guide est pour toi si tu n'as jamais ouvert un terminal ni touché à un réseau. Chaque étape te dit ce que tu fais, ce que tu dois voir, et quoi faire si ça ne ressemble pas.

> **Où trouver les fichiers.** Aucune version de Hearth n'est encore publiée (la 0.1.0 est en préparation). Les fichiers seront servis depuis un site du propriétaire du projet. Dans ce guide, son adresse est écrite `<ADRESSE-DES-VERSIONS>` : remplace-la par l'adresse annoncée à la publication. Tant qu'elle n'existe pas, les commandes qui la contiennent ne peuvent pas marcher. Tu peux construire Hearth toi-même : voir le [README](../../README.md).

## Ce dont tu as besoin

- Un **PC Windows** (Windows 10 ou plus récent, en 64 bits).
- Un **serveur Linux allumé**, sur la même box Internet (le même réseau) que ton PC. Le processeur doit être de type x86_64, ce qui est le cas de presque tous les PC et serveurs. Les processeurs de type arm (par exemple certains petits ordinateurs) ne sont pas pris en charge pour l'instant.
- Le mot de passe d'un utilisateur du serveur qui a le droit d'administrer (celui avec lequel tu installes des choses).
- **15 minutes**.

## Les mots à connaître

- **Serveur** : un ordinateur qui reste allumé pour rendre service aux autres. Ici, c'est celui que tu veux surveiller.
- **Adresse IP** : le numéro de téléphone d'un appareil sur ton réseau. Elle ressemble à `192.168.1.20`. Avec elle, ton PC sait quel appareil appeler.
- **Port** : la porte d'entrée précise d'un appareil. Une adresse IP, c'est l'immeuble ; le port, c'est le numéro de l'appartement. Hearth utilise la porte `7341`.
- **Terminal** : une fenêtre noire ou bleue où tu donnes des ordres à l'ordinateur en les écrivant, au lieu de cliquer. Tu écris une ligne, tu appuies sur Entrée, l'ordinateur répond.
- **SSH** : une façon de prendre le terminal d'un autre ordinateur à distance, depuis ton PC, par le réseau. C'est comme s'asseoir devant le serveur sans te déplacer.
- **Empreinte** : une courte suite de lettres et de chiffres qui sert de carte d'identité à ton serveur. Hearth en montre huit groupes de quatre caractères. Si l'empreinte vue par ton PC est la même que celle vue sur le serveur, tu parles bien à ton serveur et pas à un imposteur.
- **Administrateur** : un compte qui a tous les droits dans Hearth (créer des comptes, mettre à jour...). L'autre type de compte, « Lecture seule », peut regarder mais ne peut rien changer.

## Étape 1. Trouver l'adresse de ton serveur

**Ce que tu fais.** Tu as besoin de l'adresse IP du serveur. Deux façons :

- Si tu as un clavier et un écran branchés sur le serveur, fais l'étape 2 en premier, puis reviens ici : tu y trouveras l'adresse avec la commande de l'encadré ci-dessous.
- Sinon, ouvre l'espace d'administration de ta box (souvent `192.168.1.1` dans un navigateur), puis cherche la liste des appareils connectés. Repère ton serveur par son nom, et note l'adresse qui va avec.

Dans le terminal du serveur, la commande suivante affiche l'adresse :

```sh
hostname -I
```

- `hostname` : « donne-moi le nom de cette machine ».
- `-I` (un i majuscule) : « donne plutôt ses adresses IP ».

**Ce que tu dois voir.** Une ou plusieurs adresses, par exemple `192.168.1.20`. Prends celle qui commence par `192.168.` ou `10.`. Note-la sur un papier.

**Si ça ne ressemble pas.** Rien ne s'affiche, ou une suite très longue avec des deux-points : prends seulement celle qui est faite de quatre nombres séparés par des points.

## Étape 2. Ouvrir un terminal sur le serveur

**Voie A : depuis ton PC Windows, avec SSH** (le plus pratique).

1. Ouvre le menu Démarrer de Windows, tape `PowerShell`, ouvre « Windows PowerShell ». C'est le terminal de Windows.
2. Écris la ligne suivante, en remplaçant `marie` par le nom de ton utilisateur sur le serveur, et l'adresse par celle de l'étape 1, puis appuie sur Entrée :

   ```sh
   ssh marie@192.168.1.20
   ```

   - `ssh` : « prends le terminal d'un autre ordinateur ».
   - `marie@192.168.1.20` : « l'utilisateur `marie` sur la machine d'adresse `192.168.1.20` ».
3. À la première fois, le terminal pose une question en anglais qui finit par `(yes/no/[fingerprint])?`. C'est l'empreinte propre à SSH, rien à voir avec celle de Hearth. Écris `yes` puis Entrée.
4. Il demande ton mot de passe. Tape-le : **rien ne s'affiche pendant que tu écris**, c'est normal. Valide avec Entrée.

**Voie B : avec un clavier et un écran branchés sur le serveur.** Allume l'écran, connecte-toi avec ton nom et ton mot de passe. Tu arrives sur un terminal.

**Ce que tu dois voir.** Une ligne qui finit par `$`, avec un curseur qui clignote après. Elle commence souvent par `marie@nom-du-serveur`. Le terminal attend tes ordres.

**Si ça ne ressemble pas.**

- `Connection timed out` ou `No route to host` : l'adresse est fausse, ou le serveur est éteint, ou il n'est pas sur le même réseau. Recommence l'étape 1.
- `Connection refused` : le serveur est allumé mais SSH n'est pas activé dessus. Utilise la voie B.
- `Permission denied` : mauvais nom d'utilisateur ou mauvais mot de passe.
- `ssh n'est pas reconnu` : ta version de Windows n'a pas SSH. Utilise la voie B.

## Étape 3. Installer l'agent sur le serveur

L'**agent** est le petit programme de Hearth qui tourne sur le serveur. Le client Windows lui parle.

**Ce que tu fais.** Dans le terminal du serveur, écris cette ligne en une seule fois, puis Entrée (remplace `<ADRESSE-DES-VERSIONS>` comme expliqué en haut de ce guide) :

```sh
curl -fsSL <ADRESSE-DES-VERSIONS>/install.sh | sudo sh -s -- --url <ADRESSE-DES-VERSIONS>/hearth-agent-linux-x86_64
```

Ce que chaque morceau veut dire :

- `curl` : « va chercher un fichier sur Internet ».
- `-fsSL` : quatre réglages collés. `f` : « arrête-toi si ça échoue » ; `s` : « sois discret » ; `S` : « mais dis-moi quand même s'il y a une erreur » ; `L` : « suis les redirections ».
- `<ADRESSE-DES-VERSIONS>/install.sh` : le fichier à aller chercher, le programme d'installation.
- `|` : « passe ce que tu as récupéré à la commande suivante ».
- `sudo` : « fais-le avec les droits d'administrateur ». Il te demandera peut-être ton mot de passe.
- `sh` : « exécute ce programme d'installation ».
- `-s --` : « ce qui suit s'adresse au programme d'installation, pas à `sh` ».
- `--url <ADRESSE-DES-VERSIONS>/hearth-agent-linux-x86_64` : « voici où télécharger l'agent lui-même ». L'adresse doit commencer par `https://` : le script refuse sinon. Le script télécharge aussi la somme de contrôle du fichier (un fichier du même nom avec `.sha256` ajouté), et il refuse d'installer si l'agent téléchargé ne correspond pas.

Le script te pose ensuite trois questions. Tu tapes la réponse puis Entrée :

1. `Numéro de port` : appuie simplement sur Entrée pour garder `7341`.
2. `Nom du compte administrateur:` : choisis le nom de ton compte Hearth, par exemple `marie`. De 3 à 32 caractères : minuscules, chiffres, `-` et `_`. Ce compte est différent de ton utilisateur du serveur.
3. `Mot de passe:` puis `Confirme le mot de passe:` : choisis un mot de passe pour ce compte. Il faut au moins 12 caractères, avec au moins une majuscule, une minuscule et un chiffre, et il ne doit pas contenir ton identifiant. Là encore, rien ne s'affiche quand tu écris. Note-le dans un gestionnaire de mots de passe ou sur papier.

**Ce que tu dois voir.** Quelques lignes de progression (`Vérification de l'architecture système...`, `Vérification de la disponibilité du port...`, `Installation en cours...`, `Démarrage du service...`), puis :

```
Installation réussie. L'agent démarre automatiquement avec ton serveur.

Empreinte du serveur : XXXX XXXX XXXX XXXX XXXX XXXX XXXX XXXX
Note cette empreinte. ...
Adresse à saisir dans le client : nom-du-serveur:7341
```

**Note cette empreinte** (les huit groupes de quatre caractères). Pourquoi ? À l'étape 5, Hearth te montrera une empreinte. Si elle est identique, tu es sûr de parler à ton vrai serveur. Si elle est différente, quelqu'un d'autre répond à la place de ton serveur, ou tu t'es trompé d'adresse. Tu peux la relire plus tard sur le serveur avec `sudo hearth-agent fingerprint`.

L'agent démarre tout seul à chaque allumage du serveur : tu n'as plus rien à lancer.

**Si ça ne ressemble pas.**

- `Droits d'administration requis...` : tu as oublié `sudo`, ou ton utilisateur n'a pas le droit de l'utiliser.
- `Cette architecture n'est pas prise en charge...` : ton serveur n'est pas de type x86_64. Hearth ne peut pas être installé dessus.
- `Le port configuré est déjà utilisé...` : un autre programme occupe déjà le port `7341`. Relance la commande en ajoutant `--port 7342` à la fin (et utilise `7342` partout où ce guide dit `7341`).
- `L'adresse ... n'est pas en HTTPS...` ou `Aucune somme SHA-256...` ou `Aucune version de l'agent n'est publiée...` : l'adresse donnée n'est pas la bonne, ou la version n'est pas encore publiée. Vérifie `<ADRESSE-DES-VERSIONS>`.
- `curl: command not found` : l'outil `curl` manque sur ton serveur. Dis-le sur la page d'aide (voir la fin du guide).
- Si le script s'arrête avec un message, il remet la machine comme avant : tu peux relancer sans crainte.
- Ton serveur fonctionne sous NixOS, ou sans « systemd » ? L'installation existe dans une version dite « gérée », où c'est le système qui lance l'agent. Elle est expliquée dans [`docs/runbooks/installer-agent.md`](../runbooks/installer-agent.md).

## Étape 4. Installer Hearth sur Windows

**Ce que tu fais.**

1. Sur ton PC, télécharge l'installateur depuis `<ADRESSE-DES-VERSIONS>` : un fichier nommé `Hearth_<version>_x64-setup.exe`.
2. Double-clique dessus.

**À savoir : l'avertissement de Windows.** L'installateur de Hearth n'est pas encore signé par un certificat Windows : le dépôt ne configure aucun certificat de ce genre. (Les mises à jour de Hearth, elles, sont signées avec une clé que l'application vérifie elle-même ; c'est autre chose.) Windows peut donc afficher une fenêtre bleue « Windows a protégé votre ordinateur » (SmartScreen), qui dit que l'éditeur est inconnu. Cet avertissement n'est pas un virus détecté : c'est Windows qui ne connaît pas encore l'éditeur. Si, et seulement si, tu as téléchargé le fichier depuis `<ADRESSE-DES-VERSIONS>`, clique sur « Informations complémentaires », puis sur « Exécuter quand même ». Si tu as un doute sur l'origine du fichier, ne l'ouvre pas.

3. L'assistant d'installation s'ouvre en français. Sur la première page, on te dit : « Cet assistant installe Hearth pour toi seul, sans droits administrateur. » Il y a une case « Lancer Hearth au démarrage de Windows », décochée au départ. Coche-la si tu veux que Hearth s'ouvre tout seul, réduit près de l'horloge, à chaque ouverture de session. Tu pourras changer d'avis plus tard dans les réglages. Clique sur Suivant.
4. Suis les pages, en gardant l'emplacement proposé. Si on te dit qu'il faut un composant appelé WebView2, laisse-le s'installer.
5. À la fin : « Hearth a été installé sur ton ordinateur. » Clique sur Fermer.

**Ce que tu dois voir.** Hearth dans le menu Démarrer de Windows.

**Si ça ne ressemble pas.**

- `Hearth a besoin de Windows 10 ou plus récent, en 64 bits.` : ton Windows est trop ancien.
- `Il manque de la place. Libère au moins 50 Mo.` : libère de l'espace sur ton disque.
- `Hearth est en cours d'exécution. Ferme l'application avant de réessayer.` : ferme Hearth, puis relance l'installateur.

## Étape 5. Ajouter ton serveur et te connecter

**Ce que tu fais.**

1. Ouvre Hearth depuis le menu Démarrer. Un écran « Bienvenue dans Hearth » te propose « Ajoute ton premier serveur pour commencer. » Clique sur « Ajouter un serveur ».
2. Une fenêtre en trois étapes (Adresse, Empreinte, Connexion) s'ouvre. Remplis :
   - « Nom du serveur » : le nom que tu veux, par exemple `Salon`.
   - « Adresse IP ou nom » : l'adresse notée à l'étape 1, par exemple `192.168.1.20`. (Si la fin de l'installation te donnait un nom suivi de `:7341` et que le nom ne marche pas, utilise l'adresse IP.)
   - « Port (optionnel) » : laisse vide, sauf si tu as changé le port à l'étape 3.
   - « Couleur » : à ton goût.
   Clique sur « Suivant ». « Vérification en cours… » s'affiche un instant.
3. La page « Vérifie l'identité du serveur » montre « Empreinte du serveur : » avec huit groupes de quatre caractères. **Compare, groupe par groupe, avec l'empreinte notée à l'étape 3.**
   - Tout est identique : clique sur « Confirmer ».
   - Quelque chose diffère, même un seul caractère : **clique sur « Refuser » et ne continue pas.** Rien n'est enregistré. Une différence veut dire que ce n'est pas ton serveur qui répond (mauvaise adresse, ou pire, un appareil qui se fait passer pour lui). Vérifie l'adresse, relis l'empreinte sur le serveur avec `sudo hearth-agent fingerprint`, et recommence.
4. La page « Connecte-toi » demande l'« Identifiant » et le « Mot de passe » du compte créé à l'étape 3 (pas ceux de ton utilisateur du serveur). La case « Se souvenir de moi sur ce PC » garde le mot de passe dans le coffre de Windows (le Gestionnaire d'identification), pas dans un fichier. Clique sur « Se connecter ».

**Ce que tu dois voir.** « Connecté à Salon. » (avec le nom que tu as choisi), puis un bouton « Ouvrir ». En haut à droite d'un écran de ton serveur, une pastille « Connecté ».

**Si ça ne ressemble pas.** Voir la partie « Ça ne marche pas » plus bas.

## Étape 6. Premiers pas

Dans la colonne de gauche du serveur, quatre pages : « Tableau de bord », « Comptes », « Journal d'activité », « Sécurité ».

- **Tableau de bord.** C'est la santé de ta machine : processeur, mémoire, disques, carte graphique, réseau, températures et « Durée de fonctionnement ». Les boutons « 1 min », « 5 min » et « 1 h » changent la durée des courbes. Une carte qui dit « Non disponible sur cette machine » veut dire que le matériel ne donne pas cette mesure : ce n'est pas une panne.
- **Créer un compte pour quelqu'un.** Va sur « Comptes », clique sur « Ajouter un compte ». Dans la fenêtre « Créer un compte », écris un « Identifiant » (par exemple `camille`), un mot de passe (que tu confirmes dans « Confirme le mot de passe du compte »), et choisis le « Rôle » : « Administrateur » (tous les droits) ou « Lecture seule » (regarde sans rien changer). Clique sur « Créer ». Un message dit « Compte camille créé. » Hearth peut te redemander ton mot de passe pour confirmer : c'est normal pour un acte d'administration. Donne à la personne son identifiant, son mot de passe, l'adresse du serveur, et ce guide.
- **Journal d'activité.** Une ligne par événement important : qui s'est connecté, quelle action, depuis quelle adresse, « Réussi » ou « Refusé ». Tu peux chercher, filtrer, et « Exporter ». Si tu vois des « Connexion refusée » en nombre que tu ne comprends pas, va voir la page « Sécurité ».
- **Sécurité.** Tu y vois « Tes postes de confiance » (les PC reconnus pour ton compte) et le « Mode attaque » (qui n'accepte plus que les PC reconnus, à activer seulement si Hearth t'avertit d'une « Attaque probable détectée »).

Plus tard, l'icône d'engrenage en bas à gauche ouvre les « Réglages » (démarrage avec Windows, notifications, version, vérification des mises à jour).

## Ça ne marche pas

Les messages ci-dessous sont ceux de Hearth, mot pour mot.

| Ce que tu vois | Ce que ça veut dire | Ce que tu fais |
|---|---|---|
| « Cette adresse n'est pas joignable. Vérifie l'adresse et essaie de nouveau. » | Ton PC n'arrive pas à atteindre le serveur : mauvaise adresse, serveur éteint, serveur sur un autre réseau, ou **pare-feu qui bloque le port 7341** | Voir ci-dessous, « Le serveur est-il joignable ? » |
| « Cette adresse ne répond pas comme un agent Hearth. Vérifie l'adresse. » | Quelque chose répond à cette adresse, mais ce n'est pas l'agent (autre appareil, autre port) | Vérifie l'adresse et le port. Sur le serveur, `systemctl status hearth-agent` doit dire `active (running)` |
| « Serveur hors ligne. Dernier contact à … Nouvelle tentative automatique en cours. » (pastille « Hors ligne ») | Le lien est coupé : serveur éteint ou redémarré, réseau coupé | Attends : Hearth réessaie tout seul, puis la pastille revient à « Connecté ». Tu peux cliquer sur « Réessayer maintenant ». Si ça dure, regarde si le serveur est allumé |
| « L'identité de ce serveur a changé. La connexion est suspendue tant que tu n'as pas tranché. » (page « Identité du serveur changée ») | L'empreinte du serveur n'est plus celle que tu avais confirmée | **Ne continue pas tant que tu ne comprends pas pourquoi.** Clique sur « Ne pas se connecter ». Si tu viens de réinstaller l'agent ou de changer de machine, c'est normal : compare l'« Empreinte reçue » avec celle affichée sur le serveur (`sudo hearth-agent fingerprint`), et seulement si elles sont identiques clique sur « Accepter la nouvelle empreinte ». Sinon, ne l'accepte pas : quelqu'un d'autre répond peut-être à la place de ton serveur |
| « Identifiant ou mot de passe incorrect. » | Le compte ou le mot de passe est faux | Retape-les. Attention : il faut ceux du compte **Hearth** créé à l'étape 3, pas ceux de ton utilisateur du serveur. Majuscules et minuscules comptent |
| « Trop de tentatives. Attends … s avant de réessayer. » | Trop d'essais ratés d'affilée : Hearth te fait patienter | Attends le temps indiqué, puis réessaie sans te précipiter |
| « Les versions du client et de l'agent ne sont pas compatibles. Mets à jour l'agent. » (ou « …Mets à jour le client. ») | Le client et l'agent sont de versions trop différentes | Mets à jour celui qui est indiqué |
| « Session expirée » | Ta session de connexion est terminée | Clique sur « Me reconnecter » et retape ton mot de passe |
| « Accès révoqué » | Ton compte a été supprimé ou désactivé, ou son mot de passe a changé | Demande à un administrateur de rétablir ton compte |

**Le serveur est-il joignable ?** Teste dans l'ordre :

1. Le serveur est-il allumé, et branché au même réseau que ton PC ?
2. L'adresse est-elle la bonne ? Un serveur peut changer d'adresse après un redémarrage de la box. Refais l'étape 1. Pour que l'adresse ne bouge plus, la plupart des box permettent de « réserver » une adresse pour un appareil.
3. L'agent tourne-t-il ? Sur le serveur : `systemctl status hearth-agent` (`systemctl` pilote les services de la machine, `status` demande l'état, `hearth-agent` est le nom du service). Tu dois lire `active (running)`.
4. Le port est-il bloqué ? Un pare-feu est un videur qui refuse des portes. L'installateur de Hearth n'en modifie aucun. Si le serveur a son propre pare-feu, il faut y autoriser le port `7341`. Si tu utilises `ufw`, la commande est `sudo ufw allow 7341/tcp` : `ufw` est le pare-feu, `allow` « autorise », `7341/tcp` désigne la porte 7341 en mode TCP, le mode que Hearth utilise. Depuis ton PC, tu peux tester la porte dans PowerShell :

   ```sh
   Test-NetConnection 192.168.1.20 -Port 7341
   ```

   `Test-NetConnection` demande à Windows de frapper à une porte, `-Port 7341` désigne laquelle. Cherche la ligne `TcpTestSucceeded : True` : la porte est ouverte. `False` : quelque chose la bloque (pare-feu du serveur, de la box, ou autre adresse).

**Mot de passe de l'administrateur oublié ?** Il existe une procédure sur le serveur : [`docs/runbooks/recuperer-acces-administrateur.md`](../runbooks/recuperer-acces-administrateur.md).

## Désinstaller proprement

**Le client, sur Windows.**

1. Dans Hearth, page « Mes serveurs » (icône de serveur, en bas à gauche), supprime les serveurs que tu ne veux plus voir : « Supprimer », puis confirme. Hearth précise : « Ses identifiants mémorisés seront aussi supprimés. »
2. Ouvre les paramètres de Windows, « Applications », « Applications installées », trouve Hearth, puis « Désinstaller ». Une case est proposée : « Tout effacer : supprimer aussi mes serveurs enregistrés et mes mots de passe mémorisés ». Décochée, tu gardes tout (utile si tu comptes réinstaller). Cochée, les dossiers du client sont effacés. L'entrée de démarrage de Windows est retirée dans les deux cas.
3. Point à connaître : **cette case n'efface pas encore les identifiants mémorisés dans le coffre de Windows**, malgré son texte (c'est un suivi connu du projet). Si tu avais coché « Se souvenir de moi sur ce PC » et que tu veux tout retirer : ouvre le menu Démarrer, tape « Gestionnaire d'identification », ouvre « Informations d'identification Windows », puis supprime les entrées dont le nom commence par `Hearth/`. C'est pour cela qu'il vaut mieux supprimer d'abord les serveurs dans l'application (point 1), qui nettoie leurs identifiants.

**L'agent, sur le serveur.** Dans le terminal du serveur :

```sh
sudo hearth-agent uninstall
```

- `sudo` : avec les droits d'administrateur.
- `hearth-agent` : le programme de Hearth.
- `uninstall` : « désinstalle-toi ».

Il demande : « Supprimer aussi les comptes, le journal et la configuration ? (conserver/supprimer) ». Écris `conserver` pour garder tes comptes, ton journal et l'empreinte (si tu réinstalles plus tard, tout est retrouvé), ou `supprimer` pour qu'il ne reste plus rien. Tu dois voir « Service arrêté. » suivi de la phrase qui dit ce qui a été gardé ou supprimé.

## Où demander de l'aide

- Les procédures détaillées sont dans [`docs/INDEX.md`](../INDEX.md), partie « runbooks ».
- Pour poser une question ou signaler un problème : ouvre une « issue » sur la page GitHub du projet, [github.com/Voikyrioh/hearth](https://github.com/Voikyrioh/hearth). Donne le message exact que tu vois, l'étape où tu en es et ta version de Windows. **Ne colle jamais un mot de passe.** L'empreinte du serveur, elle, n'est pas secrète.
