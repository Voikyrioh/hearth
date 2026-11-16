# FicheCard

Molécule · `apps/desktop/src/components/molecules/FicheCard.vue`

Une carte de la grille du Catalogue (HRT-102, HRT-241, BR-FICHE-003, 033, 063, 064) : l'image du jeu (`FicheVisual`, ou son visuel neutre), le nom, un badge d'état (« Prêt à installer » ou « Bientôt »), la phrase de la fiche, puis la mémoire conseillée en une phrase avec sa preuve (fiche prête) ou la raison de l'attente (fiche bientôt). Une carte prête s'ouvre entière ; « Voir la fiche » reste le point d'entrée du clavier et ne déclenche pas l'ouverture deux fois. Une carte bientôt n'a ni bouton, ni clic, ni survol cliquable, et est atténuée. Pas de bouton « Installer » (un seul chemin, depuis la fiche). Les textes viennent de la fiche et sont montrés tels quels (`safeText`).

- Props : `card` (`FicheCard`), `serverId`, `serverName`, `capacityBytes` (`number | null` : la mémoire de la machine, sans elle la phrase ne la cite pas)
- Événements et slots : `open`
- Notes : Posé par `pages/Catalogue.vue`. Phrase de mémoire : `fiche/memory.ts::memorySentence`. Tests : `components/molecules/FicheCard.test.ts`, `pages/Catalogue.test.ts`.
- Revue UX du 2026-10-24 (constat 21) : toute la carte prête ouvre la fiche (curseur main, fond au survol).
