# UpdateBanner

Organisme · `apps/desktop/src/components/organisms/UpdateBanner.vue`

Bandeau discret en haut de la fenêtre (monté par `App.vue`, au-dessus de tout le contenu) pour la mise à jour du client (BR-UPDATE-003). Selon `useUpdatesStore().banner` : **annonce** (« Nouvelle version disponible » + numéro, « Notes de version », « Plus tard », « Mettre à jour maintenant »), **téléchargement** (« Téléchargement en cours… », puis « Téléchargement : 35 % »), **installation** (« Installation en cours… Redémarrage du client… »), **échec** (message exact selon `failure` : interrompu, corrompu, autre ; « Plus tard » et « Réessayer »). `role="status"`, `alert` pour un échec ; jamais modal. Il ne décide de rien : la coquille calcule `bannerVisible` (report de 24 h compris).

- Props : aucune (lit le store `updates`)
- Événements et slots : aucun
- Notes : `data-update-banner` et `data-state` (`available`, `downloading`, `installing`, `failed`) pour les tests. Tests : `components/organisms/updates.test.ts`, `e2e/updates.spec.ts`.
