# ModsApplyDialog

Organisme · `apps/desktop/src/components/organisms/ModsApplyDialog.vue`

« Vérifiez avant d'appliquer » (HRT-263, BR-MOD-025, 026, 031, maquette Confirmer révisée) : ce qui va changer (ajouts, remplacements, retraits, remises, désactivations, réactivations), les étapes dans l'ordre (deux seulement pour un serveur arrêté, qui le reste), la carte des joueurs connectés qui seront déconnectés (`PlayersOnlineCard`, sans chiffre quand la console est muette), la durée et le retour automatique, la taille de la sauvegarde d'avant, la phrase de BR-MOD-009 et le mot de passe (`AdminActPanel`, acte `mods_apply` jamais couvert par l'élévation). Espace insuffisant : l'espace qui manque est dit et le bouton reste inactif. Tenu par `composables/useModsApply.ts` ; règles pures `services/modsApply.ts`.

- Props : `serverId`, `batch`, `rows`, `running`, `apply`, `players`, `playersRefreshing`, `now`, `consoleOpen`, `blockers`
- Événements : `back`, `refreshPlayers`, `openConsole`, `launched`
- Notes : deux colonnes (changements et ordre à gauche ; joueurs, durée, confirmation à droite), un seul groupe de boutons ; `blockers` = mods qui ne démarreront toujours pas après le lot ; le texte d'arrêt dit ce que fait l'image (le jeu enregistre son monde en s'arrêtant, HRT-292 non livré). Tests `pages/ServicesModsApply.test.ts`, e2e `e2e/mods.spec.ts`.

Ordre des boutons de la maquette (« Appliquer et redémarrer » puis « Revenir à la liste »), phrase des joueurs dans la carte, étape 1 selon la fiche (`stopWay`).
