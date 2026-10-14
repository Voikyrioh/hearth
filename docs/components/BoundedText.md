# BoundedText

Atome · `apps/desktop/src/components/atoms/BoundedText.vue`

Texte venu du serveur dont la hauteur est bornée (jeton `--bounded-max-height`, 160 px par défaut) : au-delà il défile dans sa zone, qui prend alors le focus clavier et un nom lisible (`label`). L'appelant le rend inoffensif avant (`safeText`). Utilisé par le détail d'un service (nom, image, commande, chemins, noms et valeurs de variables). Test de mise en page : `e2e/hrt63-detail.spec.ts` (64 Ko).
