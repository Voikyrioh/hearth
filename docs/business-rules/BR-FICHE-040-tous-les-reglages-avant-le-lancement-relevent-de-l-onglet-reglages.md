---
id: BR-FICHE-040
domaine: FICHE
titre: « Voir tous les réglages » déplie la liste complète dans l'installation, et les réglages changés par rapport aux valeurs proposées sont comptés dans le récapitulatif
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-040), HRT-102
maj: 2026-10-24
---

# BR-FICHE-040

## Règle
Règle partagée entre HRT-102 et HRT-109. Le récapitulatif compte les réponses qui diffèrent des valeurs proposées (« {n} réglage(s) différent(s) des valeurs proposées ») ; les secrets saisis comptent et restent masqués. Sous la carte des questions, « Voir tous les réglages » (replié par défaut) déplie DANS la page la même liste que l'onglet « Réglages » (mêmes lignes, mêmes thèmes, même recherche), sur les valeurs proposées : sans pied collant, sans fichier brut, sans les réglages essentiels déjà posés plus haut ni ceux que la fiche impose (leur carte). Un réglage touché part avec l'installation comme une réponse (l'agent les juge comme les questions) ; un réglage non touché garde la valeur proposée.

## Application (code)
- `apps/desktop/src/fiche/answers.ts::changedAnswers`
- `apps/desktop/src/components/organisms/FicheSummaryDialog.vue`
- `apps/desktop/src/components/organisms/InstallSettings.vue`, `apps/desktop/src/fiche/answers.ts::{extraSettings, extraAnswersToSend, changedExtras}`, `apps/desktop/src/fiche/draft.ts::toInstallRequest`

## Vérification
- `apps/desktop/src/fiche/fiche.test.ts::ce qui diffère de ce que la fiche propose (BR-FICHE-040)`
- `apps/desktop/src/pages/InstallSettings.test.ts::un réglage changé part avec l'installation (BR-FICHE-040, BR-FICHE-016)`

## Cas limites
- Un réglage figé à la création du monde porte la mention de BR-FICHE-016.
- Vider un réglage de texte qui a une valeur proposée envoie une réponse vide (le jeu écrit une ligne vide) ; retrouver la valeur proposée se fait par « Remettre la valeur proposée ».
- L'import d'un texte collé attend un service installé (BR-FICHE-052).

## Règles liées
BR-FICHE-014, BR-FICHE-016

## Historique
- 2026-10-24 — création (HRT-102).
- 2026-10-24 — « Voir tous les réglages » livré (HRT-109).
