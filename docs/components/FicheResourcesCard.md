# FicheResourcesCard

Organisme · `apps/desktop/src/components/organisms/FicheResourcesCard.vue`

La carte « Ressources » de la fiche d'un jeu (HRT-102, BR-FICHE-008, 020, 021, 023) : processeur, mémoire et espace disque contre minimum, recommandé et « Ce serveur », chaque ressource avec sa preuve (`ProofTag`). Une valeur sans preuve n'a aucun nombre : « Non vérifié ». Sous le tableau, les constats en mots : machine trop petite pour un minimum mesuré (erreur), chiffre de l'éditeur au-dessus de la machine (avertissement), minimum non vérifié (information) et, seulement quand la valeur recommandée est prouvée, si la machine la couvre.

- Props : `detail` (`FicheDetail`), `capacity` (`EngineCapacity | null`)
- Événements et slots : `source` (`cpu`, `memory` ou `disk` : ouvrir la source d'un chiffre de l'éditeur)
- Notes : Règles pures : `fiche/fit.ts`. Tests : `pages/FicheDetail.test.ts`.
