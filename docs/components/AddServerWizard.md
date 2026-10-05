# AddServerWizard

Organisme · `apps/desktop/src/components/organisms/AddServerWizard.vue`

Assistant d'ajout d'un serveur en 3 temps (adresse, empreinte, connexion) dans une carte de 520 px. La logique est dans `composables/useAddServer.ts` ; le composant affiche. « Refuser » et « Annuler » rendent la main par `cancel` ; rien n'existe avant la connexion réussie (le serveur est enregistré par `add_and_login`), annuler ou fermer ne défait donc rien. BR-CONN-001, 002, 004, 008, 011, 012.

- Props : aucune
- Événements et slots : événements `cancel`, `done` (identifiant du serveur)
- Notes : Tests : `connect.test.ts`, `useAddServer.test.ts`, `e2e/connect.spec.ts`.
