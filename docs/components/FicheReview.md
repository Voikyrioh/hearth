# FicheReview

Organisme · `apps/desktop/src/components/organisms/FicheReview.vue`

« Vérifiez avant de lancer » (HRT-102, BR-FICHE-025, 026, 036) : la troisième étape de la PAGE d'installation (HRT-246, plus une fenêtre), pour tout ce qui part ET la confirmation du socle (`AdminActPanel`, posé sur `useAdminAct`, genre `service_deploy`, jamais couverte par l'élévation). La fiche et « Livrée avec Hearth », la variante, le nom du service, l'image exacte (version, empreinte, registre), les téléchargements, les ports et qui peut les joindre, les dossiers, les réponses (secrets masqués, « valeur proposée », nombre de réglages changés), les ressources avec la preuve de la recommandation, le redémarrage, l'arrêt du jeu, ce que la fiche impose, la phrase de confiance (jamais « isolé » ni « sûr »), puis les points d'attention qui s'appliquent, chacun une case. Les ports et les dossiers sont ceux de la variante choisie ; un port que le jeu annonce (`same_number`) se montre « {port} vers {port} ». Le contrat de licence a sa fenêtre (`AgreementDialog`), ouverte avant le mot de passe.

- Props : `serverId`, `serverName`, `host`, `detail`, `draft`, `questions`, `capacity`, `check`, `forceLowDisk`, `agentVersion`, `perform(adminPassword, accept)`
- Événements et slots : `leave` (« Retour » : l'étape « Mémoire et processeur »), `launched` (l'envoi est parti), `edit` (« Modifier les réglages »), `agree` (les contrats acceptés), `imposed`
- Notes : Règles pures : `fiche/attention.ts`. Tests : `pages/InstallFromFiche.test.ts`.

HRT-109 : `extras` (les réglages de « Voir tous les réglages ») : ceux qui diffèrent de la valeur proposée sont listés après les réponses et comptés dans « {n} réglage(s) différent(s) des valeurs proposées ».

HRT-246 : sections « Le serveur de jeu », « Ce qui va se passer » (quatre étapes, `fiche/review.ts::whatWillHappen`), « L'adresse prévue pour vos joueurs » (`plannedAddress`), points d'attention, « Votre mot de passe Hearth ». La barre du bas porte « Retour », la raison qui retient le lancement (`launchReason`) et « Installer le serveur de jeu ». Le contrat de licence garde sa fenêtre à deux boutons (`AgreementDialog`). Tests : `pages/InstallFromFiche.test.ts`, `fiche/review.test.ts`.
- Revue UX du 17/11 : trois points d'attention de plus, à cocher avant le lancement (`fiche/attention.ts`) : les réglages gravés dans le monde à sa création, le processeur que la fiche ne prouve pas, l'absence de mot de passe. La phrase sur l'enregistrement du monde est celle de la ligne « Arrêt du jeu » (`fiche/messages.ts::stopTexts`) : une seule vérité. La barre n'est plus collée en bas : elle ne cache plus l'erreur du mot de passe.
