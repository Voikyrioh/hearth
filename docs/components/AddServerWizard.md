# AddServerWizard

Organisme · `apps/desktop/src/components/organisms/AddServerWizard.vue`

Assistant d'ajout d'un serveur en 3 temps (adresse, empreinte, connexion) dans une carte de 520 px. La logique est dans `composables/useAddServer.ts` ; le composant affiche. « Refuser » et « Annuler » rendent la main par `cancel` ; rien n'existe avant la connexion réussie (le serveur est enregistré par `add_and_login`), annuler ou fermer ne défait donc rien. BR-CONN-001, 002, 004, 008, 011, 012.

- Props : aucune
- Événements et slots : événements `cancel`, `done` (identifiant du serveur)
- Notes : Tests : `connect.test.ts`, `useAddServer.test.ts`, `e2e/connect.spec.ts`.

HRT-33 (revue UX du 2026-10-08) : le curseur est dans le premier champ de chaque étape qui en a un (nom, puis identifiant) ; « Suivant » n'est jamais grisé : un champ manquant ou faux est dit sous le champ et le curseur y va, au clic comme à Entrée (FIX-01M4D0RJ5EMX3TG1TJB0EJ5EYP). Tests : `connect.test.ts`, `e2e/hrt33-keyboard.spec.ts`.
- HRT-40 (C3) : l'étape « Empreinte » rappelle le nom et l'adresse du serveur, offre « Précédent », dit où relire l'empreinte (`sudo hearth-agent fingerprint`) et que « Refuser » annule l'ajout.
