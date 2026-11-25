# ModsProgress

Organisme · `apps/desktop/src/components/organisms/ModsProgress.vue`

Le suivi d'un lot de mods et son issue (HRT-263, BR-MOD-025 à 027, maquettes Suivi, Fin, Echec) : les étapes cochées au fil du flux (quittable : l'opération continue sur le serveur), « Annuler l'opération » jusqu'au début de la mise en place ; la fin réussie (ce qui a changé, la sauvegarde faite, « Pensez à vos joueurs ») ; l'échec revenu à l'état d'avant (les dernières lignes du journal, la phrase en clair quand l'agent la reconnaît, aucun choix « Revenir » ni « Laisser tel quel ») ; l'annulation et l'état « à vérifier ». Les lignes du journal sont du texte du jeu, brutes, jamais du HTML.

- Props : `serverId`, `apply`, `view`
- Événements : `close`, `players`, `logs`
- Notes : tests `pages/ServicesModsApply.test.ts`, e2e `e2e/mods.spec.ts`.

Étapes terminées : coche verte et mot « Terminé ». L'étape 1 redit la fiche (`stopWay`), comme la carte « Arrêt du jeu », la fenêtre « Arrêter » et le refus de `stop` de la console.
