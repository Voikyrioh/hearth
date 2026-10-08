# AccountTable

Organisme · `apps/desktop/src/components/organisms/AccountTable.vue`

Tableau des comptes (administrateurs) : Identifiant (étiquette « toi » sur sa ligne), Rôle (Administrateur en contour braise), Créé le, Dernière connexion (« Jamais »), Sessions ouvertes, actions. Autre ligne : « Changer le rôle » (liste déroulante inline), « Mot de passe », « Fermer les sessions » (« Fermer les <n> sessions » dès deux, grisé à zéro), « Supprimer » (contour critique). Sa ligne : « Changer mon mot de passe » seulement. Le dernier administrateur : « Changer le rôle » et « Supprimer » grisés avec l'explication. Toute action porte `needsLink` avec le rôle administrateur (BR-RESIL-008) ; l'agent reste l'arbitre.

- Props : `accounts`, `meId` (identifiant de l'AGENT du compte de la session, jamais un texte comparé), `busy` (une action est en cours : aucune autre ligne ne part)
- Événements et slots : Événements `changeRole(account, role)`, `changePassword(account)`, `changeOwnPassword`, `closeSessions(account)`, `remove(account)`
- Notes : Défilement horizontal interne si étroit. BR-ACCT-007 à 012. Test : `accounts.test.ts`, `e2e/accounts.spec.ts`.
- HRT-44 : la cellule du rôle en cours de changement a un fond et un trait (`td[data-editing]`, C22) ; l'en-tête garde une seule police (C26) ; quand « Créé le » est masquée (tableau sous 1100 px), l'identifiant se focalise (`tabindex=0`) et la date apparaît au survol et au focus, décrite au lecteur d'écran (`aria-describedby`, `accounts.createdOn`). FIX:01M4DNJ3P75DN25714EBCH2EY2, FIX:01M4DNJ3W899Y50NPSRQ8W8ZVN. Test : `e2e/hrt43-44.spec.ts`.
- Date de création (revue du 2026-10-08) : colonne « Créé le » masquée (tableau sous 1100 px) : « Créé le jj/mm/aaaa » en petit texte atténué sous l'identifiant, ni arrêt de tabulation ni `aria-describedby` ; colonne visible : rien sous l'identifiant (jamais dite deux fois).
- HRT-42 : le tableau est borné à `--table-max` (1 400 px). FIX:01M4EDC0FVT0ZHGASFEMWX7066. Décision de Claude, à confirmer par Voiky (conception grand écran, décisions 3 à 6).
- Seconde passe (D6) : Échap sur la liste du rôle rend le curseur au bouton « Changer le rôle ». FIX:01M4EHYPD658X22RH4SFV0TDVE.
