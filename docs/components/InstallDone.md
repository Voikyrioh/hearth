# InstallDone

Organisme · `apps/desktop/src/components/organisms/InstallDone.vue`

L'écran de fin d'une installation depuis une fiche (HRT-247, BR-FICHE-066) : « Le serveur de jeu « {nom} » est en marche. » (ou « démarre. » avec la durée annoncée quand la fiche déclare un premier démarrage long), le monde créé quand la fiche en nomme un, la carte d'adresse du détail (`GameAddressCard` : réseau maison et internet, « Copier »), la carte des ports à ouvrir (`GamePortsCard` : port, protocole, rôle), la console à distance dite non publiée quand la fiche en déclare une, les étapes de l'installation repliées (`ProgressSteps`), « Fermer » et « Ouvrir le serveur de jeu ». Les adresses et les ports sont lus sur l'agent (`useGameView`) ; une lecture qui échoue se dit avec « Réessayer ».

- Props : `serverId`, `serviceKey`, `name`, `world` (`null` si la fiche n'en nomme pas), `detail`, `operation` (la dernière image de l'opération), `withWorld`
- Événements : `open` (le détail du service créé), `close` (la liste des services)
- Notes : posé par `pages/InstallFromFiche.vue` quand `useDeployFlow.onSucceeded` rend `"stay"`. Aucune valeur de réglage ni mot de passe : la saisie est effacée au lancement. Tests : `components/organisms/installDone.test.ts`, `e2e/ajouter-un-serveur-de-jeu.spec.ts`.
