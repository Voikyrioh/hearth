# AddServer

Page · `apps/desktop/src/pages/AddServer.vue`

Ajout d'un serveur : carte centrée sans navigation de serveur. Route `/servers/new` (toujours permise). Annuler ou refuser l'empreinte ramène à l'accueil ; une connexion réussie ouvre le tableau de bord du nouveau serveur.

- Props : aucune
- Événements et slots : aucun
- Notes : Tests : `e2e/connect.spec.ts`.
- HRT-248 : le bouton qui ouvre l'assistant dit « Relier un serveur » (barre de gauche, carnet des serveurs, accueil), pour ne plus se confondre avec « Ajouter un serveur de jeu » de la page Services ; titre « Relier un serveur à ce PC ».
