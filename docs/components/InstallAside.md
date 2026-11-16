# InstallAside

Molécule · `apps/desktop/src/components/molecules/InstallAside.vue`

L'encart de résumé de l'étape « Réglages » (HRT-244, BR-FICHE-065) : le jeu et le serveur qui l'accueillera, la phrase de la fiche, la mémoire en une phrase avec sa preuve (`fiche/memory.ts::memorySentence`), l'adresse prévue pour les joueurs (adresse du serveur et port du jeu choisi) et la promesse « Rien n'est créé avant l'étape « Vérifiez avant de lancer ». ». Il ne calcule rien : les phrases lui sont données par la page.

- Props : `heading`, `summary`, `memory`, `address` (`null` : aucune ligne d'adresse, la fiche ne publie aucun port)
- Notes : à côté du formulaire, il passe dessous en fenêtre étroite (`pages/InstallFromFiche.vue`, `flex-wrap`). Test : `pages/InstallFromFiche.test.ts`.
