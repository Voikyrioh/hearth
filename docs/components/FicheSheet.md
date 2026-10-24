# FicheSheet

Organisme · `apps/desktop/src/components/organisms/FicheSheet.vue`

Les cartes de lecture de la fiche d'un jeu (HRT-102, BR-FICHE-004, 008, 010) : Image (dépôt, version, empreinte, registre), Ports, Dossiers (chemin dans le service), Ressources, Espace disque, Téléchargements (et durée du premier démarrage, étapes reconnues), Arrêt du jeu, Questions (genre, valeur proposée, bornes, choix, mention de figé), Ce que cette fiche impose (compte et bouton) et, quand il y en a un, Contrat de licence. Tout est lu dans la fiche ; aucune ligne « compte exigé ».

- Props : `detail` (`FicheDetail`), `capacity` (`EngineCapacity | null`)
- Événements et slots : `source`, `agreement` (identifiant du contrat à lire), `imposed` (ouvrir la liste)
- Notes : Posé par `pages/FicheDetail.vue`. Tests : `pages/FicheDetail.test.ts`.

HRT-109 : la carte « Questions » dit « {n} réglages, dont {m} demandés à l'installation » et les thèmes de la fiche quand l'agent sert le catalogue complet.
