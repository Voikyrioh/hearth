# Dashboard

Page · `apps/desktop/src/pages/Dashboard.vue`

Tableau de bord d'un serveur : sa machine en direct (HRT-11). Suit le serveur courant (`useDashboardStore().follow`), puis montre, selon l'état : le chargement (« Chargement des mesures », lien connecté sans identité reçue), « Aucune mesure pour l'instant » (jamais joint, lien non connecté) ou « Les mesures de ce serveur ne peuvent pas être lues pour le moment » (abonnement en échec), sinon la grille de 12 colonnes dans `StaleSurface` (dernières valeurs grisées et datées quand le lien n'est pas « Connecté », BR-DASH-009). Sélecteur de durée des courbes (1 min, 5 min par défaut, 1 h) au-dessus de la grille. Mêmes cartes pour les deux rôles (BR-DASH-013).

- Props : aucune
- Événements et slots : aucun
- Notes : Route `/servers/:id/dashboard`. Grille PAR CONTENEUR (largeur de la page, pas de la fenêtre) : au-dessus de 1300 px, 12 colonnes : Machine (3) + Durée de fonctionnement, Processeur (5), Mémoire (4) + Températures empilée dessous ; Carte graphique (5), Réseau (4), Disques (3) ; de 700 à 1300 px, 6 colonnes (Machine + Processeur, Mémoire + Réseau, Carte graphique et Disques pleine largeur) ; sous 700 px, une colonne. Les cartes d'une rangée ont la même hauteur. Un chargement de plus de 3 s dit « Mesures en cours de chargement… » et propose « Réessayer » (HRT-34, FIX-01M4D1K9GV5X6MJTHDB8RYPMS6). Tests : `pages/Dashboard.test.ts`, `e2e/dashboard.spec.ts`.
