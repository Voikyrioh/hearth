# ConsoleWaitSection

Molécule · `apps/desktop/src/components/molecules/ConsoleWaitSection.vue`

« Ce qui attend la console » (HRT-109, BR-FICHE-051), repliée : chaque fonction qui attend la console est nommée, avec un bouton présent et inactif et « Cette fonction attend la console de Hearth, qui n'est pas encore livrée. » suivi du contournement. Trois fonctions valent pour tout jeu ; celles qui sont propres à un jeu viendront de la fiche.

- Props : aucune
- Événements et slots : aucun
- Notes : Tests : `components/organisms/gameSettingsTab.test.ts`.
- HRT-269 : prop `served` (l'agent parle à la console de ce jeu) : la section ne garde que « Appliquer un réglage sans redémarrer » et dit « Ouvrir la console » (événement `openConsole`). Test `components/molecules/consoleWait.test.ts`.
