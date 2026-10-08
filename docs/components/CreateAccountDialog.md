# CreateAccountDialog

Organisme · `apps/desktop/src/components/organisms/CreateAccountDialog.vue`

Création d'un compte : « Identifiant », « Mot de passe » (avec `PasswordRules`), « Confirme le mot de passe », « Rôle » (Lecture seule par défaut, le moindre privilège). Validation en direct par `useAccountRules` (commande `check_account_input`) ; « Créer » inerte tant que tout n'est pas valide. Envoi par `useAccountActions.create` : refus « identifiant déjà utilisé » sous le champ, formulaire conservé ; mots de passe vidés après chaque envoi et à la fermeture.

- Props : `open`, `serverId`
- Événements et slots : Événement `close`
- Notes : BR-ACCT-001 à 005. Test : `accounts.test.ts`.

HRT-30 : habillée par `AdminActDialog` (kind `account_create`, rôle visé = rôle choisi : donner « Administrateur » n'est jamais couvert par le délai) ; la saisie est gardée si le délai se ferme entre-temps.

HRT-33 : un refus (mot de passe de confirmation faux, attente, délai fermé, identifiant pris) GARDE la saisie du nouveau compte, seule la confirmation est à refaire (C18, FIX-01M4D0RJB17YE0F26QFXGJ2WEV). Les mots de passe du nouveau compte ne sont vidés qu'après un envoi non refusé. Libellés : « Mot de passe du nouveau compte », « Confirme le mot de passe du compte », exemple d'identifiant « ex. camille », « Ton mot de passe » en dernier (C19, FIX-01M4D0RJJNZK7NGDMSFBE18Q29).
