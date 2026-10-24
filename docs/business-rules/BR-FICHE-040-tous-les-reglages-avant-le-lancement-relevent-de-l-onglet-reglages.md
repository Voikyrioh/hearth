---
id: BR-FICHE-040
domaine: FICHE
titre: Les réglages changés par rapport aux valeurs proposées sont comptés dans le récapitulatif ; « Voir tous les réglages » relève de HRT-109
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-040), HRT-102
maj: 2026-10-24
---

# BR-FICHE-040

## Règle
Règle partagée entre HRT-102 et HRT-109. Le récapitulatif compte les réponses qui diffèrent des valeurs proposées (« {n} réglage(s) différent(s) des valeurs proposées ») ; les secrets saisis comptent et restent masqués. Le dépliant « Voir tous les réglages » et l'import de réglages ne sont PAS dans HRT-102 : ils arrivent avec l'onglet Réglages (HRT-109).

## Application (code)
- `apps/desktop/src/fiche/answers.ts::changedAnswers`
- `apps/desktop/src/components/organisms/FicheSummaryDialog.vue`

## Vérification
- `apps/desktop/src/fiche/fiche.test.ts::ce qui diffère de ce que la fiche propose (BR-FICHE-040)`

## Cas limites
- Un réglage figé à la création du monde porte la mention de BR-FICHE-016.

## Règles liées
BR-FICHE-014, BR-FICHE-016

## Historique
- 2026-10-24 — création (HRT-102).
