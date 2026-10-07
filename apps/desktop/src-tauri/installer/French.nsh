; Textes français (tutoiement) de l'installateur Hearth. Remplace le fichier fourni par
; Tauri, qui vouvoie. Mêmes clés que le modèle ; seules les formulations changent.
LangString addOrReinstall ${LANG_FRENCH} "Ajouter ou réinstaller un composant."
LangString alreadyInstalled ${LANG_FRENCH} "Déjà installé."
LangString alreadyInstalledLong ${LANG_FRENCH} "${PRODUCTNAME} ${VERSION} est déjà installé. Choisis l'opération à effectuer, puis clique sur Suivant. Tes serveurs et tes réglages sont conservés."
LangString appRunning ${LANG_FRENCH} "{{product_name}} est en cours d'exécution. Ferme l'application avant de réessayer."
LangString appRunningOkKill ${LANG_FRENCH} "{{product_name}} est en cours d'exécution.$\nClique sur OK pour fermer l'application."
LangString chooseMaintenanceOption ${LANG_FRENCH} "Choisis l'option de maintenance à effectuer."
LangString choowHowToInstall ${LANG_FRENCH} "Choisis l'emplacement d'installation de ${PRODUCTNAME}."
LangString createDesktop ${LANG_FRENCH} "Créer un raccourci sur le bureau."
LangString dontUninstall ${LANG_FRENCH} "Ne pas désinstaller"
LangString dontUninstallDowngrade ${LANG_FRENCH} "Ne pas désinstaller (revenir à une ancienne version sans désinstaller est désactivé pour cet installateur)"
LangString failedToKillApp ${LANG_FRENCH} "Impossible de fermer {{product_name}}. Ferme l'application et réessaie."
LangString installingWebview2 ${LANG_FRENCH} "Installation de WebView2..."
LangString newerVersionInstalled ${LANG_FRENCH} "Une version plus récente de ${PRODUCTNAME} est déjà installée. Installer une ancienne version n'est pas recommandé. Si tu veux quand même le faire, désinstalle d'abord la version actuelle. Choisis l'opération à effectuer, puis clique sur Suivant."
LangString older ${LANG_FRENCH} "ancien"
LangString olderOrUnknownVersionInstalled ${LANG_FRENCH} "La version $R4 de ${PRODUCTNAME} est installée sur ton PC. Il est recommandé de la désinstaller avant d'installer celle-ci. Choisis l'opération à effectuer, puis clique sur Suivant."
LangString silentDowngrades ${LANG_FRENCH} "Revenir à une version antérieure est désactivé pour cet installateur. L'installation silencieuse ne peut pas continuer, utilise l'interface graphique à la place.$\n"
LangString unableToUninstall ${LANG_FRENCH} "Impossible de désinstaller le programme !"
LangString uninstallApp ${LANG_FRENCH} "Désinstaller ${PRODUCTNAME}"
LangString uninstallBeforeInstalling ${LANG_FRENCH} "Désinstaller avant d'installer"
LangString unknown ${LANG_FRENCH} "inconnu"
LangString webview2AbortError ${LANG_FRENCH} "L'installation de WebView2 a échoué. Hearth ne peut pas fonctionner sans WebView2. Relance l'installation."
LangString webview2DownloadError ${LANG_FRENCH} "Erreur : le téléchargement de WebView2 a échoué - $0"
LangString webview2DownloadSuccess ${LANG_FRENCH} "WebView2 a été téléchargé."
LangString webview2Downloading ${LANG_FRENCH} "Téléchargement de WebView2..."
LangString webview2InstallError ${LANG_FRENCH} "Erreur : l'installation de WebView2 a échoué avec le code $1"
LangString webview2InstallSuccess ${LANG_FRENCH} "WebView2 est installé."
; Case de la page de désinstallation, décochée par défaut : décochée = on garde tout.
; À FAIRE avec le coffre Windows (Gestionnaire d'identification) : le modèle de Tauri ne
; supprime que des dossiers. Quand les mots de passe y seront stockés, la désinstallation
; devra aussi effacer ces identifiants si la case est cochée, sinon ce texte ment.
LangString deleteAppData ${LANG_FRENCH} "Tout effacer : supprimer aussi mes serveurs enregistrés et mes mots de passe mémorisés"
; Contrôles d'avant installation (BR-CLIENT-014), appelés par hooks.nsh.
LangString hearthWindowsTooOld ${LANG_FRENCH} "Hearth a besoin de Windows 10 ou plus récent, en 64 bits. Ta version ne fonctionne pas."
LangString hearthDiskTooSmall ${LANG_FRENCH} "Il manque de la place. Libère au moins 50 Mo. Espace disponible : $R8 Mo."
; Lancement au démarrage de Windows (HRT-21, BR-CLIENT-006), case de la page d'accueil, décochée par défaut.
LangString hearthAutostartLabel ${LANG_FRENCH} "Lancer Hearth au démarrage de Windows"
LangString hearthAutostartHelp ${LANG_FRENCH} "Hearth s'ouvre tout seul quand tu ouvres ta session, réduit près de l'horloge. Tu pourras changer ce choix à tout moment dans les réglages de l'application."
LangString hearthWelcomeText ${LANG_FRENCH} "Cet assistant installe ${PRODUCTNAME} pour toi seul, sans droits administrateur.$\r$\n$\r$\nClique sur Suivant pour continuer."
LangString hearthFinishText ${LANG_FRENCH} "${PRODUCTNAME} a été installé sur ton ordinateur.$\r$\n$\r$\nClique sur Fermer pour quitter le programme d'installation."
