---
id: BR-FICHE-065
domaine: FICHE
titre: L'étape « Réglages » montre l'essentiel à l'écran et replie le reste dans « Réglages avancés »
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-ajouter-un-serveur-de-jeu.md (BR-FICHE-065), HRT-244
maj: 2026-11-16
---

# BR-FICHE-065 — L'essentiel d'abord, les réglages avancés repliés

## Règle
L'étape « Réglages » montre à l'écran : la variante s'il y en a, le nom du service, la question qui nomme le monde, les questions secrètes, les questions sans valeur proposée, et les ports. Elle montre le bloc « Le monde » quand la fiche sait reprendre un monde. Toutes les autres questions sont dans « Réglages avancés », replié, avec leurs valeurs proposées ; le bloc s'ouvre seul quand l'une de ses questions est refusée (par le client ou par le serveur) ou désignée par « Aller au champ ». L'ordre de la fiche est conservé dans les deux listes.

## Application (code)
- `apps/desktop/src/fiche/questions.ts::{splitQuestions, advancedLabels}`
- `apps/desktop/src/components/organisms/InstallAnswers.vue` (bloc `<details>` piloté, `focusQuestion` async)

## Vérification
- `apps/desktop/src/fiche/questions.test.ts`
- `apps/desktop/src/pages/InstallFromFiche.test.ts::montre l'essentiel et replie le reste dans « Réglages avancés » (BR-FICHE-065)`

## Règles liées
- BR-FICHE-014, BR-FICHE-016, BR-FICHE-060

## Historique
- 2026-11-16 — création (HRT-244).
