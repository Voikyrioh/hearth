# ServerRow

Organisme · `apps/desktop/src/components/organisms/ServerRow.vue`

Une ligne du carnet : avatar, nom (lien vers le tableau de bord), adresse, mention « Identifiants mémorisés », mention « Mise à jour disponible » du seul serveur dont l'agent a une version plus récente (BR-UPDATE-023), pastille d'état du lien de CE serveur, actions « Se déconnecter » (connecté seulement), « Oublier mes identifiants » (si mémorisés), « Modifier », « Supprimer ». BR-CONN-015.

- Props : `server`
- Événements et slots : événements `edit`, `remove`, `disconnect`, `forget`
- Notes : Tests : `Servers.test.ts`.
- HRT-26 : une étiquette écrite après le nom (« Mode attaque », « Mode attaque suspendu », « Alerte de sécurité ») et la marque sur l'avatar. Test : `pages/SecurityMode.test.ts`.
- Seconde passe (D8) : la pastille d'état suit le nom, les étiquettes passent à la ligne. FIX:01M4EHYPMVHR2TY39G3BQ7ZJZA.
