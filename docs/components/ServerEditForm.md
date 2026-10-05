# ServerEditForm

Organisme · `apps/desktop/src/components/organisms/ServerEditForm.vue`

Modification d'un serveur (logique dans `composables/useEditServer.ts`) : nom, couleur, adresse, port. Une autre adresse passe par la sonde puis « Vérifie l'identité du serveur » avant d'enregistrer (BR-CONN-009) ; les identifiants mémorisés sont conservés.

- Props : `server`
- Événements et slots : événements `done`, `cancel`
- Notes : Affiché en place de la ligne par `Servers`. Tests : `Servers.test.ts`.
