# FicheSummaryDialog

Organisme · `apps/desktop/src/components/organisms/FicheSummaryDialog.vue`

« Vérifiez avant de lancer » (HRT-102, BR-FICHE-025, 026, 036) : une fenêtre large pour tout ce qui part ET la confirmation du socle (`AdminActDialog`, genre `service_deploy`, jamais couverte par l'élévation). La fiche et « Livrée avec Hearth », la variante, le nom du service, l'image exacte (version, empreinte, registre), les téléchargements, les ports et qui peut les joindre, les dossiers, les réponses (secrets masqués, « valeur proposée », nombre de réglages changés), les ressources avec la preuve de la recommandation, le redémarrage, l'arrêt du jeu, ce que la fiche impose, la phrase de confiance (jamais « isolé » ni « sûr »), puis les points d'attention qui s'appliquent, chacun une case. Les ports et les dossiers sont ceux de la variante choisie ; un port que le jeu annonce (`same_number`) se montre « {port} vers {port} ». Le contrat de licence a sa fenêtre (`AgreementDialog`), ouverte avant le mot de passe.

- Props : `open`, `serverId`, `detail`, `draft`, `questions`, `capacity`, `check`, `forceLowDisk`, `agentVersion`, `perform(adminPassword, accept)`
- Événements et slots : `close`, `agree` (les contrats acceptés), `imposed`
- Notes : Règles pures : `fiche/attention.ts`. Tests : `pages/InstallFromFiche.test.ts`.

HRT-109 : `extras` (les réglages de « Voir tous les réglages ») : ceux qui diffèrent de la valeur proposée sont listés après les réponses et comptés dans « {n} réglage(s) différent(s) des valeurs proposées ».
