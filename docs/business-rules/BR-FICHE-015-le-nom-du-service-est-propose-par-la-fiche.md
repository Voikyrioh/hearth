---
id: BR-FICHE-015
domaine: FICHE
titre: Le nom du service est proposé par la fiche (le nom du jeu en minuscules sans accent), suivi de -2, -3… s'il est pris
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-015), HRT-102
maj: 2026-10-24
---

# BR-FICHE-015

## Règle
Règle portée par le client (HRT-102). Le nom proposé est le nom du jeu en minuscules, sans accent, réduit aux lettres, chiffres, tirets et points ; s'il est pris, le même nom suivi de « -2 », « -3 »… jusqu'au premier libre. L'utilisateur peut le modifier ; les règles du nom d'un déploiement s'appliquent (BR-DEPLOY-003, 004) et l'agent dit en direct qu'un nom est pris, sans jamais dire par qui. Installer deux fois la même fiche est permis.

## Application (code)
- `apps/desktop/src/fiche/name.ts::proposedName`
- `apps/desktop/src/composables/useFicheInstall.ts`

## Vérification
- `apps/desktop/src/fiche/fiche.test.ts::le nom du service proposé (BR-FICHE-015)`
- `apps/desktop/src/pages/InstallFromFiche.test.ts::propose terraria-2 quand terraria est pris`

## Cas limites
- Les services en cours de déploiement comptent parmi les noms pris.

## Règles liées
BR-DEPLOY-003, BR-DEPLOY-004, BR-FICHE-014

## Historique
- 2026-10-24 — création (HRT-102).
