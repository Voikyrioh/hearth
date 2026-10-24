# GameSettingsTab

Organisme · `apps/desktop/src/components/organisms/GameSettingsTab.vue`

L'onglet « Réglages » d'un service de jeu (HRT-109, BR-FICHE-037 à 053), deuxième après « Détail » (`?onglet=reglages`). Tous les réglages utiles du jeu, rangés par thèmes en mots simples : les libellés, aides, bornes et choix sont ceux de la fiche, les valeurs voulues et l'état du fichier ceux de l'agent, qui juge chaque changement (tous ou aucun). Un seul bandeau à la fois, par priorité : réglage imposé défait (`ImposedBrokenBanner`), fichier illisible, fichier réécrit (`FileChangedBanner`), changements à appliquer (« Redémarrer maintenant »), fichier disparu, version du jeu inconnue. Pied collant : « Enregistrer » (acte `game_settings_save`, couvert par l'élévation, avec la case « Redémarrer maintenant » décochée), « Annuler les changements », « Importer des réglages », « Tout remettre aux valeurs proposées », « Modifier le fichier brut » (absent, avec sa phrase, pour un jeu qui réécrit son fichier). États : chargement, droit refusé, pas de réglages, fiche retirée, lecture en échec. Les valeurs saisies, mots de passe compris, vivent en mémoire seulement et s'effacent à l'issue ou au démontage.

- Props : `serverId`, `service` (`ServiceInfo`), `connected`, `engineReady`
- Événements et slots : `restart` (la page ouvre la fenêtre de confirmation habituelle d'un redémarrage, BR-JEU-035)
- Notes : État : `composables/useGameSettings.ts`. Règles pures : `settings/`. Tests : `components/organisms/gameSettingsTab.test.ts`, `e2e/hrt109-reglages.spec.ts`.
