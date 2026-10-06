# AccountTable

Organisme · `apps/desktop/src/components/organisms/AccountTable.vue`

Tableau des comptes (administrateurs) : Identifiant (étiquette « toi » sur sa ligne), Rôle (Administrateur en contour braise), Créé le, Dernière connexion (« Jamais »), Sessions ouvertes, actions. Autre ligne : « Changer le rôle » (liste déroulante inline), « Mot de passe », « Fermer les sessions » (« Fermer les <n> sessions » dès deux, grisé à zéro), « Supprimer » (contour critique). Sa ligne : « Changer mon mot de passe » seulement. Le dernier administrateur : « Changer le rôle » et « Supprimer » grisés avec l'explication. Toute action porte `needsLink` avec le rôle administrateur (BR-RESIL-008) ; l'agent reste l'arbitre.

- Props : `accounts`, `me` (identifiant du compte connecté)
- Événements et slots : Événements `changeRole(account, role)`, `changePassword(account)`, `changeOwnPassword`, `closeSessions(account)`, `remove(account)`
- Notes : Défilement horizontal interne si étroit. BR-ACCT-007 à 012. Test : `accounts.test.ts`, `e2e/accounts.spec.ts`.
