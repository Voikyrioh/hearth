---
id: FIX-01M4D0RJJNZK7NGDMSFBE18Q29
titre: Deux champs « Mot de passe » identiques à la création d'un compte
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D0RJJNZK7NGDMSFBE18Q29 : Deux champs « Mot de passe » identiques à la création d'un compte

## Symptôme
Le mot de passe du nouveau compte et celui de l'administrateur portaient le même texte (« Mot de passe ») ; l'identifiant proposait « ton-identifiant » pour le compte de quelqu'un d'autre (C19).

## Reproduction
Test rouge avant correctif : `accounts.test.ts` « names whose password each field asks for ».

## Cause root
Libellés et exemples hérités du formulaire de connexion.

## Impacté
Client Windows, écrans concernés, depuis leur livraison. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Cliquer d'abord dans le bon champ.

## Correction
« Mot de passe du nouveau compte » (exemple « Celui que ce compte utilisera »), « Confirme le mot de passe du compte », exemple d'identifiant « ex. camille », « Ton mot de passe » (exemple « Le tien, pour confirmer »). Écart assumé avec les libellés de la spec fonctionnelle de la création (« Mot de passe », « Confirme le mot de passe ») : la revue UX le demande, à confirmer par Voiky au smoke.

## Règles
- Aucune règle métier touchée (interface seulement).

## Non-régression
- `accounts.test.ts`, `e2e/accounts.spec.ts`, `e2e/reauth.spec.ts`.

## Références
- Ticket : HRT-33 (lot 1)
