# CatalogToolbar

Molécule · `apps/desktop/src/components/molecules/CatalogToolbar.vue`

La barre du Catalogue (HRT-241, BR-FICHE-063) : recherche (`HInput` de type recherche, 60 caractères, Échap la vide), filtres `HSegmented` avec le nombre de cartes visibles (« Tous les jeux », « Prêts à installer », « Bientôt » seulement s'il y a une fiche qui attend) et « J'ai déjà un serveur de jeu ». Elle passe à la ligne en fenêtre étroite ; le texte d'un bouton ne passe jamais à la ligne (HRT-249).

- Props : `counts`, `showSoon`, `disabled` ; modèles `query`, `filter`
- Événements et slots : `haveServer`
- Notes : Règles pures `catalogue/sections.ts`. Tests : `pages/Catalogue.test.ts`.
