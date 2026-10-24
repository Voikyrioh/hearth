---
id: BR-FICHE-014
domaine: FICHE
titre: Le formulaire d'installation a trois étapes : « Réponses » et « Ressources » sont des pages, « Vérifiez avant de lancer » une fenêtre large ; aucun champ libre
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-014), HRT-102
maj: 2026-10-24
---

# BR-FICHE-014

## Règle
Règle de présentation, portée par le client (HRT-102). « Réponses » (variante, nom du service, questions de la fiche, ports) et « Ressources » sont des pages ; « Vérifiez avant de lancer » est la fenêtre large qui porte la confirmation du socle (BR-FICHE-026). Revenir à une étape précédente ne perd rien. Il n'y a aucun champ libre pour une variable, une commande, une image ou un dossier : ce que la fiche ne demande pas n'est pas modifiable. La variante est choisie en tête de « Réponses » (BR-FICHE-060), le contrat de licence dans sa fenêtre (BR-FICHE-058).

## Application (code)
- `apps/desktop/src/pages/InstallFromFiche.vue`
- `apps/desktop/src/components/organisms/InstallAnswers.vue`
- `apps/desktop/src/components/organisms/InstallResources.vue`
- `apps/desktop/src/components/organisms/FicheSummaryDialog.vue`

## Vérification
- `apps/desktop/src/pages/InstallFromFiche.test.ts::la page (BR-FICHE-014, 015)`
- `apps/desktop/src/pages/InstallFromFiche.test.ts::revient à l'étape « Réponses » sans rien perdre`

## Cas limites
- « Voir tous les réglages » (BR-FICHE-040) et l'import de réglages relèvent de HRT-109 : non faits ici.
- La demande envoyée à l'agent ne porte ni image, ni variable, ni commande, ni dossier (`link/catalog.ts::FicheInstallRequest`).

## Règles liées
BR-FICHE-013, BR-FICHE-026, BR-FICHE-038, BR-FICHE-058, BR-FICHE-060

## Historique
- 2026-10-24 — création (HRT-102).
