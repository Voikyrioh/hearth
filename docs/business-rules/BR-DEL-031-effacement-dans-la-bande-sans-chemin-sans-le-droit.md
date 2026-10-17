---
id: BR-DEL-031
domaine: DEL
titre: L'effacement des données d'un service se suit dans la bande ; sans le droit, une phrase sans chemin ni progression
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-031), HRT-84
maj: 2026-10-17
---

# BR-DEL-031 — L'effacement des données se suit dans la bande, sans chemin pour qui n'a pas le droit

## Règle
Pendant un effacement de données, et après la fermeture de sa fenêtre, l'opération se retrouve dans la bande « Opérations en cours » (BR-SVC-029), y compris quand le service n'a plus de ligne de liste ni de détail : le nom retenu à la première vue reste affiché. Un compte sans le droit de gérer les services lit « Effacement des données de {nom} en cours. », sans étape, sans jauge et sans chiffres ; un compte avec le droit voit la progression (son texte détaillé vient avec la suppression, HRT-85). La suppression prend le verrou exclusif du service (BR-LIFE-013).

## Application (code)
- `apps/desktop/src/serviceOps/messages.ts::operationTitle` (phrase sans chemin quand `detailed` est faux)
- `apps/desktop/src/serviceOps/messages.ts::progressOf` (aucune jauge ni chiffre sans le droit)
- `apps/desktop/src/components/organisms/OperationsStrip.vue` (`sentenceOnly`)
- `apps/desktop/src/stores/serviceOps.ts::nameOf` (nom retenu à la première vue)

## Vérification
- `apps/desktop/src/serviceOps/messages.test.ts`
- `apps/desktop/src/pages/ServicesOperations.test.ts` (quatre profils)

## Cas limites
- Le chemin géré et le compte des éléments effacés ne viennent qu'avec la suppression (HRT-85) ; aujourd'hui le pont simulé seul produit cette opération.

## Règles liées
- BR-SVC-029, BR-LIFE-013

## Historique
- 2026-10-17 — création (HRT-84, la bande côté client).
