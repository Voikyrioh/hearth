# AddServerWizard

Organisme · `apps/desktop/src/components/organisms/AddServerWizard.vue`

Assistant d'ajout d'un serveur en 3 temps (adresse, empreinte, connexion) dans une carte de 520 px. La logique est dans `composables/useAddServer.ts` ; le composant affiche. « Refuser » et « Annuler » rendent la main par `cancel` ; fermer l'assistant au 3e temps retire le serveur enregistré. BR-CONN-001, 002, 004, 008, 011, 012.

- Props : aucune
- Événements et slots : événements `cancel`, `done` (identifiant du serveur)
- Notes : Tests : `connect.test.ts`, `useAddServer.test.ts`, `e2e/connect.spec.ts`.
