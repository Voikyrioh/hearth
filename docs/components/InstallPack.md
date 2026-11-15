# InstallPack

Organisme · `apps/desktop/src/components/organisms/InstallPack.vue`

Le pack de mods de l'installation d'un serveur de jeu (HRT-125, BR-MOD-006 à 009, 016) : la case « Installer avec un pack de mods » (jamais cochée d'avance), le choix du fichier, et ce que l'agent a LU dans le contenu du fichier (le genre « Pack serveur » ou « Pack Modrinth » n'est jamais un champ à remplir). Le moteur de serveur et la version de Minecraft se choisissent dans les réponses : la carte dit seulement où les lire quand le pack ne les annonce pas, et l'écran choisit d'office le seul moteur que la fiche permet pour le genre lu. Un refus se dit en phrases (une liste CurseForge : trois phrases, ce que c'est, quoi télécharger à la place, l'autre voie par Modrinth), jamais avec un nom d'entrée de l'archive. La carte n'existe que si la fiche offre le dépôt d'un pack (`FicheDetail.mods`).

- Props : `install` (`FicheInstallState`, le résultat de `useFicheInstall`)
- Événements et slots : aucun
- Envoi : « Choisir un fichier » ouvre la boîte « Ouvrir » du système (`useTransfersStore().pick()`), puis la fenêtre d'acte d'écriture « Envoyer le pack au serveur » (`AdminActDialog`, mot de passe selon la règle de l'agent) lance l'envoi par transfert reprenable (`start`, HRT-95 et HRT-96). L'identifiant du transfert est celui du dépôt : quand le transfert est fini, la carte fait lire le pack (`inspectPack`). Un envoi arrêté ou annulé retire le fichier choisi.
