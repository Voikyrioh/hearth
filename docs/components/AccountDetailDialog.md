# AccountDetailDialog

Organisme · `apps/desktop/src/components/organisms/AccountDetailDialog.vue`

Détail d'un compte en lecture seule (HRT-78), ouvert par le nom dans `AccountTable` : rôle, dates, sessions et `ServicesRightRow`. Ne propose que « Fermer » ; le focus revient au nom. Ajout du ticket : le client n'avait pas de détail de compte.

- Props : `account` (`null` : fermé), `rightsKnown` ; événement `close`
- Tests : `pages/AccountsRights.test.ts`, `e2e/services-rights.spec.ts`.
