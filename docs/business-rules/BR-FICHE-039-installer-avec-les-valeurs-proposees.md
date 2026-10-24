---
id: BR-FICHE-039
domaine: FICHE
titre: « Installer avec les valeurs proposées » ouvre directement « Vérifiez avant de lancer », sans accepter aucun contrat
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-039), HRT-102
maj: 2026-10-24
---

# BR-FICHE-039

## Règle
Règle portée par le client (HRT-102). Le second bouton de la fiche ouvre l'installation avec le nom proposé, toutes les valeurs proposées par la fiche et les ressources recommandées, puis la fenêtre « Vérifiez avant de lancer » dès que la fiche et les bornes de la machine sont lues. Un champ à corriger (un jeton de serveur obligatoire, par exemple) ramène à la page. Il n'accepte aucun contrat de licence : sa fenêtre s'ouvre quand même à « Installer le serveur de jeu » (BR-FICHE-058).

## Application (code)
- `apps/desktop/src/pages/FicheDetail.vue`
- `apps/desktop/src/pages/InstallFromFiche.vue`

## Vérification
- `apps/desktop/src/pages/InstallFromFiche.test.ts::« Installer avec les valeurs proposées » (BR-FICHE-039)`
- `apps/desktop/src/pages/FicheDetail.test.ts::« Installer » (BR-FICHE-012, 033, 039)`

## Cas limites
- Le choix de la mise à jour automatique du jeu relève de HRT-128 : il n'est pas dans cette version.

## Règles liées
BR-FICHE-014, BR-FICHE-058

## Historique
- 2026-10-24 — création (HRT-102).
