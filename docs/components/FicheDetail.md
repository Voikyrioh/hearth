# FicheDetail

Page · `apps/desktop/src/pages/FicheDetail.vue`

La fiche d'un jeu (HRT-102) : `FicheSheet`, les constats de la machine, la phrase de confiance, l'empreinte de la fiche, « Installer » (seul bouton principal) et « Installer avec les valeurs proposées ». Boutons actifs pour un compte qui a le droit, lien connecté, moteur prêt, machine assez grande ; sinon visibles et désactivés avec la raison (moteur inutilisable : absents). Une fiche inconnue de l'agent est dite telle quelle ; une fiche que ce client ne sait pas lire en entier ne s'installe pas d'ici.

- Props : —
- Événements et slots : —
- Notes : Route `/servers/:id/catalogue/:fiche` (nom `fiche`). Ouvre les adresses de la fiche par la coquille (`openFicheLink`). Tests : `pages/FicheDetail.test.ts`.
