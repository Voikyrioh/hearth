# ModsDepositDialog

Organisme · `apps/desktop/src/components/organisms/ModsDepositDialog.vue`

« Ajouter des mods » (HRT-262, BR-MOD-015, 022, maquette Depot) : choisir des fichiers `.jar` (8 au plus) par la boîte « Ouvrir » du système, les envoyer dans la zone d'attente (un transfert reprenable chacun, mot de passe seulement si l'agent l'exige), voir l'état de chaque envoi, puis « Contrôler les fichiers » quand tous sont finis. Un fichier refusé avant l'envoi (pas un `.jar`, neuvième fichier, illisible) dit pourquoi. Tenu par `composables/useModsDeposit.ts`. Rien n'est écrit sur le serveur.

- Props : `serverId`, `deposit` (retour de `useModsDeposit`), `canWrite`, `connected`
- Événements : `back`
- Notes : écart à la maquette : pas de glisser-déposer (la coquille ne le relaie pas encore), la zone dit « Choisissez vos fichiers ». Tests `pages/ServicesModsBatch.test.ts`.
