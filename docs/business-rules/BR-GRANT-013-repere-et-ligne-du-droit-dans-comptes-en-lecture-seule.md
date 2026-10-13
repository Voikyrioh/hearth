---
id: BR-GRANT-013
domaine: GRANT
titre: Dans Comptes, le repère « Peut gérer les services » et la ligne « Gestion des services » avec la commande à copier sont en lecture seule
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-013), HRT-78
maj: 2026-10-13
---

# BR-GRANT-013 — Dans Comptes, le repère « Peut gérer les services » et la ligne « Gestion des services » avec la commande à copier sont en lecture seule

## Règle
La liste des comptes marque le compte qui a le droit (« Peut gérer les services »). Le détail d'un compte dit « Autorisée » ou « Non autorisée », l'aide « Ce droit se donne et se retire depuis le serveur, pas depuis cet écran. » et la commande `hearth-agent account services {compte} --grant` (`--revoke` si autorisée) avec « Copier la commande » (copie exactement le texte affiché). Aucune case ni commutateur. La commande n'est construite que pour un identifiant qui passe `isAccountName` (même validation que l'aide du moteur, HRT-74 S7) : forme `^[a-z_][a-z0-9_-]{2,31}$`, plus stricte que BR-ACCT-002 (un identifiant commençant par un chiffre ou un tiret n'a pas de commande). Agent ancien : la ligne dit de mettre le serveur à jour.

## Application (code)
- `apps/desktop/src/accounts/name.ts::isAccountName`, `services/rights.ts::grantCommand`
- `components/molecules/ServicesRightRow.vue`, `components/organisms/AccountTable.vue`, `AccountDetailDialog.vue`

## Vérification
- `accounts/name.test.ts`, `services/rights.test.ts`, `components/molecules/serviceRights.test.ts`, `pages/AccountsRights.test.ts`, `e2e/services-rights.spec.ts`

## Règles liées
BR-GRANT-005, BR-GRANT-016, BR-ACCT-002, BR-ENGINE-017

## Historique
- 2026-10-13 — création (HRT-78, moitié client).
