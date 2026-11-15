# Installer un serveur Minecraft avec un pack de mods (exemple : Prominence II)

Une fiche `minecraft` (HRT-116) installe un serveur Minecraft Java : officiel, Paper, Purpur, Fabric, Quilt, Forge ou NeoForge, avec ou sans pack de mods. Le pack se dépose depuis le client ; l'agent le lit, le range en lecture seule et passe son chemin à l'image `itzg/minecraft-server`. **Aucune voie CurseForge** : Hearth n'a aucune clé CurseForge, ne télécharge rien chez CurseForge et refuse une simple liste de téléchargements (HRT-124, BR-MOD-016, BR-MOD-017). Un pack CurseForge s'installe par son **pack serveur**, que vous téléchargez vous-même.

Statut : la fiche est publiée (HRT-116) : elle est dans l'agent de chaque version, sans construction d'essai. Le banc d'essai réel du 15/11/2026 (forge, hors Hearth, sans joueur) a vu démarrer, répondre, enregistrer le monde et s'arrêter proprement le jeu officiel et Fabric avec Prominence II ; le relevé est dans l'ADR-0053.

## Prérequis sur le serveur

1. **Un agent à jour.** La fiche Minecraft est embarquée dans l'agent publié : rien à construire (`cargo xtask agent --preview-fiches` ne sert plus à Minecraft).
2. **Le port du jeu ouvert.** Minecraft écoute en TCP sur le port 25565 du serveur (réglable à l'installation). Sur un hôte NixOS (la forge), le pare-feu doit l'accepter : `networking.firewall.allowedTCPPorts = [ 25565 ];` dans la configuration du serveur, puis une redirection du port sur la box pour jouer depuis Internet. Hearth n'ouvre jamais un port de l'hôte lui-même.
3. **La mémoire.** Le tas de la machine virtuelle Java suit la limite de mémoire du service (75 % de cette limite) : réglez le curseur « Mémoire » de l'installation. Un pack de plusieurs centaines de mods demande beaucoup : pour Prominence II, la page du pack annonce au moins 8 Go de tas, donc réglez la limite à 12 Go (le tas en est 75 %, soit 9 Go). Si la machine partage sa mémoire avec d'autres jeux ou des modèles d'IA, laissez-leur leur part.
4. **De la place.** Le fichier du pack (1,58 Go pour Prominence II 4.1.8) est rangé tel quel, puis le programme du jeu l'ouvre dans son dossier (1,83 Go ouvert), plus le monde. L'agent refuse l'installation s'il manque la place du fichier et de son contenu.
5. **Le droit « gérer les services »** pour le compte qui installe, et l'agent compatible (capacité `game-packs`).

## Étapes (côté utilisateur)

1. **Télécharger le pack serveur.** Sur la page du modpack sur CurseForge, ouvrez l'onglet des fichiers, choisissez la version, puis cherchez « Server Pack » dans les fichiers supplémentaires. Téléchargez ce fichier `.zip` et **ne l'ouvrez pas** (Prominence II 4.1.8 : fichier `Prominence_II_Hasturian_Era_4.1.8_server_pack.zip`, 1,58 Go, Fabric, Minecraft 1.20.1, Java 17 ; lien public sans compte : `https://www.curseforge.com/minecraft/modpacks/prominence-2-hasturian-era/download/9563792`). Le dossier `mods` et le dossier `config` doivent être à la racine de l'archive ; sinon Hearth refuse le pack (« Ce pack range tout dans un dossier. ») et il faut refaire l'archive. Vérifié le 15/11/2026 sur le pack 4.1.8 : `mods` (377 jars), `config`, `datapacks` et `defaultconfigs` sont à la racine, sans dossier enveloppe.
2. Dans Hearth, ouvrez **Catalogue**, puis **Minecraft Java**, puis **Installer**.
3. Dans les réponses : choisissez le moteur **Fabric** (Prominence II), mettez la **version de Minecraft** à `1.20.1` (celle de la page du pack), cochez **Installer avec un pack de mods** et choisissez le fichier. Une fenêtre vous demande de confirmer l'envoi (votre mot de passe, sauf si le délai de confirmation est ouvert) : le fichier part vers le serveur par un envoi qui reprend là où il s'est arrêté si le lien coupe, et la carte dit où il en est. Quand il est entièrement arrivé, Hearth le lit et dit ce qu'il y a trouvé : « Pack serveur », le nombre de mods, la taille. Un pack refusé est retiré aussitôt et le motif est dit en une phrase.
4. À l'étape « Ressources » : réglez la **mémoire** (12 Go pour Prominence II) et le processeur.
5. À l'étape « Vérifiez avant de lancer » : lisez que le programme du jeu copie le contenu du pack dans son dossier, que Hearth ne l'ouvre pas et ne le vérifie pas, et **acceptez le contrat de licence de Minecraft** (une fenêtre à deux boutons : l'adresse du contrat s'ouvre dans votre navigateur, Hearth n'accepte jamais à votre place). Confirmez avec votre mot de passe.
6. Le premier démarrage est long (le fichier est ouvert, le moteur Fabric téléchargé, des centaines de mods chargés) : plusieurs minutes. Le service passe « En marche » quand l'image a dit « Done ». Essai réel sur l'image épinglée (15/11/2026, hors Hearth, mémoire limitée à 7 Go) : « Done (47,5 s)! For help, type « help » » environ 2 minutes après le lancement. L'image choisit elle-même la version du moteur Fabric (0.20.1 à cette date, et non la 0.19.3 écrite dans le pack) et le serveur démarre.
7. Se connecter depuis Minecraft avec le **même pack côté joueur** (Prominence II, même version) : adresse du serveur et port 25565.

## Arrêt, sauvegarde

- L'arrêt passe par le signal de l'image, qui envoie `stop` au jeu (le monde est enregistré) et attend jusqu'à 180 secondes ; le moteur de conteneurs attend 210 secondes avant toute coupure forcée. Essai réel (15/11/2026, hors Hearth) : arrêt par signal en 23 secondes, code de sortie 0, tous les mondes enregistrés.
- Les sauvegardes de Hearth couvrent le dossier `/data` (monde, réglages du jeu, mods ouverts) et pas le dossier des packs (le pack se redépose).

## Ce que Hearth ne fait pas (encore)

- Remplacer le pack d'un service installé, ou déposer une archive de mods dans « Tes mods » : HRT-217.

## En cas de refus

| Ce que dit Hearth | Ce que c'est |
|---|---|
| « Ce fichier est la version du pack pour le lanceur de jeu… (n mods) » | Une liste de téléchargements CurseForge : téléchargez le pack serveur à la place, ou un pack Modrinth. |
| « Ce pack range tout dans un dossier. » | Le contenu du pack serveur n'est pas à la racine de l'archive. |
| « Ce pack demande de télécharger depuis {hôte}… » | Un pack Modrinth qui vise un hôte hors des quatre du format. |
| « Ce fichier n'est pas une archive que Hearth peut lire en sécurité. » | Archive illisible, hors bornes ou piégée. |
| « Ce fichier n'est plus sur le serveur. » | Le dépôt a expiré ou a été retiré : choisissez-le de nouveau. |
