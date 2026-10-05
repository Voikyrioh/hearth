# ServerRow

Organisme · `apps/desktop/src/components/organisms/ServerRow.vue`

Une ligne du carnet : avatar, nom (lien vers le tableau de bord), adresse, mention « Identifiants mémorisés », pastille d'état du lien de CE serveur, actions « Se déconnecter » (connecté seulement), « Oublier mes identifiants » (si mémorisés), « Modifier », « Supprimer ». BR-CONN-015.

- Props : `server`
- Événements et slots : événements `edit`, `remove`, `disconnect`, `forget`
- Notes : Tests : `Servers.test.ts`.
