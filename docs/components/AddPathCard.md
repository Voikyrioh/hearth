# AddPathCard

Molécule · `apps/desktop/src/components/molecules/AddPathCard.vue`

Un chemin de l'écran « Ajouter un serveur de jeu » (HRT-243, BR-DEPLOY-039) : titre, étiquette facultative, texte, note et un bouton. Bouton inactif : sa raison est écrite dessous.

- Props : `title`, `tag`, `text`, `note`, `button`, `primary`, `disabled`, `reason`
- Événements et slots : `go`
- Notes : Posé par `pages/AddGameServer.vue`. Tests : `pages/AddGameServer.test.ts`.
