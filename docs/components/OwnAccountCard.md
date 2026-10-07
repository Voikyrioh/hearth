# OwnAccountCard

Organisme · `apps/desktop/src/components/organisms/OwnAccountCard.vue`

Carte « Mon compte » d'un serveur dans les réglages (tous les rôles) : nom du serveur, rôle, « Connecté en tant que <identifiant> », « Changer mon mot de passe » et, pour un administrateur, « Supprimer mon compte » (fenêtre « Supprimer ton compte ? », retaper son identifiant, BR-ACCT-012). Les boutons exigent le lien de CE serveur (`needsLink` avec `server`).

- Props : `server`
- Événements et slots : Aucun
- Notes : BR-ACCT-009, 012, 013. Test : `accounts.test.ts`, `SettingsAccounts.test.ts`.

HRT-30 : la suppression de son compte passe par `AdminActDialog` (kind `account_delete`) EN PLUS de l'identifiant retapé.
