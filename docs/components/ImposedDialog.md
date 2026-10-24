# ImposedDialog

Organisme · `apps/desktop/src/components/organisms/ImposedDialog.vue`

Ce que la fiche d'un jeu impose (HRT-102, BR-JEU-039) : un tableau « Réglage · Valeur · Pourquoi », en lecture seule. Une valeur tirée, calculée ou secrète n'est jamais montrée : la cellule dit « Calculée par Hearth ».

- Props : `open`, `settings` (`ImposedSetting[]`)
- Événements et slots : `close`
- Notes : Posé par `FicheDetail` et par `InstallFromFiche`. Tests : `pages/FicheDetail.test.ts`, `pages/InstallFromFiche.test.ts`.
