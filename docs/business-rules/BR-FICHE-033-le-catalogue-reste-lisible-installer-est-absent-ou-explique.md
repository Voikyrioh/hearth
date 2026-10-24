---
id: BR-FICHE-033
domaine: FICHE
titre: Le Catalogue reste lisible quand le moteur n'est pas prêt ; « Installer » est absent ou désactivé avec sa raison
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-033), HRT-102
maj: 2026-10-24
---

# BR-FICHE-033

## Règle
Règle portée par le client (HRT-102). Le coupe-circuit des services ou un moteur absent, non pris en charge ou d'une version non prise en charge laisse le Catalogue lisible : « Installer » est ABSENT et l'encart du moteur est en tête. Un moteur qui n'est pas « prêt » mais existe désactive « Installer » avec la raison écrite dessous. Un compte sans le droit de gérer les services voit « Installer » désactivé avec la raison du droit (BR-FICHE-012). La route de l'installation est gardée comme le formulaire de déploiement et renvoie vers la fiche.

## Application (code)
- `apps/desktop/src/pages/FicheDetail.vue`
- `apps/desktop/src/router/deployGuard.ts::ficheInstallGuard`
- `apps/desktop/src/deploy/availability.ts::deployDoor`

## Vérification
- `apps/desktop/src/pages/FicheDetail.test.ts::« Installer » (BR-FICHE-012, 033, 039)`
- `apps/desktop/src/pages/InstallFromFiche.test.ts::renvoie vers la fiche un compte qui ne peut pas gérer les services`

## Cas limites
- Le Catalogue fonctionne sans Internet : les fiches sont livrées avec l'agent.

## Règles liées
BR-FICHE-012, BR-GRANT-014

## Historique
- 2026-10-24 — création (HRT-102).
