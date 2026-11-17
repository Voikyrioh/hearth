# FicheSheet

Organisme · `apps/desktop/src/components/organisms/FicheSheet.vue`

Les cartes de lecture de la fiche d'un jeu (HRT-102, BR-FICHE-004, 008, 010) : Image (dépôt, version, empreinte, registre), Ports, Dossiers (chemin dans le service), Ressources, Espace disque, Téléchargements (et durée du premier démarrage, étapes reconnues), Arrêt du jeu, Questions (genre, valeur proposée, bornes, choix, mention de figé), Ce que cette fiche impose (compte et bouton) et, quand il y en a un, Contrat de licence. Tout est lu dans la fiche ; aucune ligne « compte exigé ».

- Props : `detail` (`FicheDetail`), `capacity` (`EngineCapacity | null`)
- Événements et slots : `source`, `agreement` (identifiant du contrat à lire), `imposed` (ouvrir la liste)
- Notes : Posé par `pages/FicheDetail.vue`. Tests : `pages/FicheDetail.test.ts`.

HRT-109 : la carte « Questions » dit « {n} réglages, dont {m} demandés à l'installation » et les thèmes de la fiche quand l'agent sert le catalogue complet.
- Revue UX du 2026-10-24 : (constat 4) une valeur proposée se dit par son libellé (`fiche/messages.ts::valueLabel` : « Maître », « Oui », jamais le numéro de la fiche) ; (constat 19) la carte « Espace disque » disparaît quand `noFigureAtAll` : la carte Ressources le dit déjà.

HRT-221 : trois groupes, sur toute la largeur utile (grille `--sheet-col-min`) : ce qu'il faut (Ressources, Ports, Espace disque, Téléchargements, Arrêt du jeu), les réglages (Questions, Ce que cette fiche impose, Contrat), puis « Détails techniques », replié (`<details>`) : Image, Dossiers et, par le slot `technical`, l'empreinte de la fiche.
- HRT-242 : « Ce qu'il faut pour ce jeu » dit mémoire, disque, processeur, ports, premier démarrage et arrêt, réglages en phrases (`fiche/facts.ts::needLines`), avec leur preuve ; `FicheResourcesCard` (le tableau des chiffres) passe sous « Voir le tableau des chiffres et leurs preuves ». Nouvelle prop `serverName`.
- Revue UX du 17/11 : « Les réglages du jeu » liste les questions de l'installation avec leur valeur proposée et dit combien d'autres réglages se modifient ensuite ; Ports et Téléchargements sont dans le tableau replié « Voir le tableau des chiffres et leurs preuves » (avec un chevron).
