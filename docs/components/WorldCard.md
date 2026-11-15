# WorldCard

Organisme · `apps/desktop/src/components/organisms/WorldCard.vue`

La carte « Monde » de l'installation d'un serveur de jeu (HRT-106, BR-MONDE-002, 003, 006, 009, 012, 014) : « Nouveau monde » ou « Reprendre un monde existant » (désactivé avec sa raison sans dossier ouvert), la liste des mondes avec leur dossier, le monde choisi avec ce que la copie écrit et laisse, « Garder la copie si des fichiers changent », le lien vers l'aide. N'existe que si la fiche sait reprendre un monde et si l'agent a répondu.

- Props : `install` (l'état de `useFicheInstall`)
- Événements : aucun
- Notes : posée par `InstallAnswers`. Tests : `pages/InstallWorld.test.ts`.
