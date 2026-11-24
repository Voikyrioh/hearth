# ModsDepositDialog

Organisme · `apps/desktop/src/components/organisms/ModsDepositDialog.vue`

« Ajouter des mods » (HRT-262, BR-MOD-015, 022, maquette Depot) : choisir des fichiers `.jar` (8 au plus) par la boîte « Ouvrir » du système, les envoyer dans la zone d'attente (un transfert reprenable chacun, mot de passe seulement si l'agent l'exige), voir l'état de chaque envoi, puis « Contrôler les fichiers » quand tous sont finis. Un fichier refusé avant l'envoi (pas un `.jar`, neuvième fichier, illisible) dit pourquoi. Tenu par `composables/useModsDeposit.ts`. Rien n'est écrit sur le serveur.

- Props : `serverId`, `deposit` (retour de `useModsDeposit`), `canWrite`, `connected`
- Événements : `back`
- Notes : écart à la maquette : pas de glisser-déposer (la coquille ne le relaie pas encore), la zone dit « Choisissez vos fichiers ». Tests `pages/ServicesModsBatch.test.ts`.
- Notes (revue Nora 022) : l'envoi part sans fenêtre quand l'acte `file_write` ne demande pas de mot de passe ; les fichiers glissés sur la fenêtre (`bridge.onFilesDropped`, événement `tauri://drag-drop`) suivent les mêmes règles que la boîte « Ouvrir ».

Zone de dépôt de la maquette (pointillé lisible, icône d'envoi, « Glissez vos fichiers .jar ici »). L'envoi part dès le choix quand l'élévation est active (l'état de confirmation est relu avant de décider). Une fois le contrôle fait, la ligne de la carte « Transferts » est retirée. Un fichier qui a quitté la zone d'attente est nommé.
