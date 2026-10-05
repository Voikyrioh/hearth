# LoginForm

Organisme · `apps/desktop/src/components/organisms/LoginForm.vue`

Formulaire de connexion : « Identifiant », « Mot de passe », « Se souvenir de moi sur ce PC » (cochée par défaut, BR-CONN-004). Champs figés et « Connexion en cours… » pendant la tentative ; « trop de tentatives » remplace le bouton par le compte à rebours (BR-CONN-006) ; l'erreur ne dit pas lequel des champs est faux (BR-CONN-013).

- Props : `username`, `busy`, `error`, `lockedSeconds`, `submitLabel`, `backLabel`, `remember`
- Événements et slots : événements `submit` ({ username, password, remember }), `back` ; méthode exposée `clearPassword`
- Notes : Sert au 3e temps de l'assistant et à `ReconnectPanel`. Tests : `connect.test.ts`.
