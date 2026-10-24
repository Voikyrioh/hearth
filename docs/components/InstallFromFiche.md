# InstallFromFiche

Page · `apps/desktop/src/pages/InstallFromFiche.vue`

Installer un serveur de jeu depuis une fiche (HRT-102, BR-FICHE-012 à 036) : « Réponses » et « Ressources » sont des pages, « Vérifiez avant de lancer » (`FicheSummaryDialog`) une fenêtre large. Le parcours suivant (consultation fraîche, mot de passe à chaque fois, progression, issue, effacement de la saisie) est celui du déploiement : `useDeployFlow` reçoit la fiche comme source (`DeployFlowSource`), rien n'est recopié. Machine trop petite pour un minimum mesuré : dit avant toute saisie, « Continuer » désactivé sans case. `?proposees=1` ouvre la fenêtre aussitôt. Fermer avec une saisie : « Abandonner votre saisie ? ».

- Props : —
- Événements et slots : —
- Notes : Route `/servers/:id/catalogue/:fiche/installer` (nom `fiche-install`), gardée par `ficheInstallGuard`. Tests : `pages/InstallFromFiche.test.ts`.
- Revue UX du 2026-10-24 : curseur dans le nom du service à l'arrivée (constat 16) ; titre d'en-tête « Installer {jeu} » à chaque étape (constat 17) ; « Retour » ou Échap sur la fenêtre ouverte par « Installer avec les valeurs proposées », sans rien avoir saisi, ramène à la fiche d'où l'on vient (constat 20) ; rechargement avec une saisie : question native `beforeunload` (`composables/useUnloadGuard.ts`, constat 7 ; dans la coquille Tauri, Ctrl+R et F5 passent par la même question de WebView2, la croix de la fenêtre cache la page sans la décharger donc la saisie survit, BR-CLIENT-004).
