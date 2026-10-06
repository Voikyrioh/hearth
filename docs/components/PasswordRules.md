# PasswordRules

Molécule · `apps/desktop/src/components/molecules/PasswordRules.vue`

Critères du mot de passe en direct : une coche (respecté) ou une croix (non respecté) ET un texte pour les lecteurs d'écran (« Respecté » / « Non respecté »), jamais la couleur seule. Les critères viennent de l'agent (`check_account_input`, règle de `hearth-proto`) : aucune règle dans ce composant. Mot de passe vide (`required`) : aucun critère n'est « Respecté », tout est neutre (`data-met="pending"`, « Pas encore saisi »). Avant la première saisie, les critères sont en retrait (`touched` faux), sans rouge.

- Props : `unmet` (règles non respectées), `touched`
- Événements et slots : Aucun
- Notes : Textes : `fr.ts` groupe `accounts` (`rule*`, « Respecté », « Non respecté ») ; ordre : `accounts/messages.ts::PASSWORD_CRITERIA`. BR-ACCT-004, BR-ACCT-005. Test : `accounts.test.ts`, `e2e/accounts.spec.ts`.
