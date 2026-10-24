# FicheDetail

Page · `apps/desktop/src/pages/FicheDetail.vue`

La fiche d'un jeu (HRT-102) : `FicheSheet`, les constats de la machine, la phrase de confiance, l'empreinte de la fiche, « Installer » (seul bouton principal) et « Installer avec les valeurs proposées ». Boutons actifs pour un compte qui a le droit, lien connecté, moteur prêt, machine assez grande ; sinon visibles et désactivés avec la raison (moteur inutilisable : absents). Une fiche inconnue de l'agent est dite telle quelle ; une fiche que ce client ne sait pas lire en entier ne s'installe pas d'ici.

- Props : —
- Événements et slots : —
- Notes : Route `/servers/:id/catalogue/:fiche` (nom `fiche`). Ouvre les adresses de la fiche par la coquille (`openFicheLink`). Tests : `pages/FicheDetail.test.ts`.
- Revue UX du 2026-10-24 : le titre de l'en-tête est le nom du jeu (la page ne le répète pas, constat 17) ; les besoins inconnus se disent une fois (constat 19).
