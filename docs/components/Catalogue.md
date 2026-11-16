Le Catalogue (HRT-102, HRT-241, BR-FICHE-001 à 006, 033, 063, 064) : les fiches que l'agent liste, en grille illustrée (`FicheCard`) rangée en deux sections (« Prêts à installer », « Bientôt dans Hearth »), avec recherche et filtres (`CatalogToolbar`, ni l'un ni l'autre n'est gardé en quittant la page), lisibles de tous les comptes et sans Internet, jamais journalisées. En pied : « Fiches livrées avec Hearth {version} », les fiches écartées et celles que ce client ne sait pas lire. États : chargement (six cartes), lien coupé, échec avec « Réessayer », aucune fiche, recherche sans résultat, agent sans les services, moteur pas prêt (son encart en tête, la grille reste lisible).# Catalogue

Page · `apps/desktop/src/pages/Catalogue.vue`

Le Catalogue (HRT-102, BR-FICHE-001 à 006, 033) : les fiches que l'agent propose, en pile verticale (`FicheCard`), lisibles de tous les comptes et sans Internet, jamais journalisées. En pied : les fiches écartées et celles que ce client ne sait pas lire. États : chargement, lien coupé, échec avec « Réessayer », aucune fiche, agent sans les services (« Ce serveur doit être mis à jour pour proposer des fiches. »), moteur pas prêt (son encart en tête, la liste reste lisible).

- Props : —
- Événements et slots : —
- Notes : Route `/servers/:id/catalogue` (nom `catalogue`). Composables : `useFicheCatalog`, `useAgentVersion`. Tests : `pages/Catalogue.test.ts`.
