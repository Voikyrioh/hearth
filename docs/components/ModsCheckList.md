# ModsCheckList

Organisme · `apps/desktop/src/components/organisms/ModsCheckList.vue`

Le contrôle des fichiers déposés (HRT-262, BR-MOD-021 à 023, maquette Controle) : un verdict par fichier en une phrase ; un remplacement a sa case « Remplacer… » décochée d'office (non cochée, le fichier est laissé de côté) ; un fichier refusé dit seulement qu'il n'est pas compatible ou pourquoi, sans plage de versions, et se retire de la liste ; « Continuer avec N fichiers » met les ajouts dans le lot (8 au plus).

- Props : `files`, `game`, `nameOf`, `shaOf`, `room`
- Événements : `continue` (les `PendingAdd`), `more`, `drop`
- Notes : tests `pages/ServicesModsBatch.test.ts`.
