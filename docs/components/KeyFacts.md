# KeyFacts

Molécule · `apps/desktop/src/components/molecules/KeyFacts.vue`

Les quatre faits clés d'une fiche (HRT-242, BR-FICHE-004) : mémoire, espace disque, ports à ouvrir, durée du premier démarrage, une icône et une phrase chacun. Les phrases sont composées par `fiche/facts.ts::keyFacts`.

- Props : `facts` (`KeyFact[]`)
- Notes : Tests : `fiche/facts.test.ts`, `pages/FicheDetail.test.ts`.
