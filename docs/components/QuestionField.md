# QuestionField

Molécule · `apps/desktop/src/components/molecules/QuestionField.vue`

Une question d'installation d'une fiche (HRT-102, BR-FICHE-016, 017, 059) rendue dans le champ de son genre : texte, nombre entier (champ numérique sans molette), choix (boutons exclusifs jusqu'à quatre choix, sinon liste), oui ou non (deux boutons, les valeurs sont celles du jeu), secret (champ masqué, « Afficher / Masquer »). Libellé, aide, bornes, choix et mention « Ne pourra plus être changé après la création du monde » viennent de la fiche : aucune règle de jeu ici. Un genre inconnu devient un champ de texte et l'agent juge. Un mot de passe de jeu laissé vide avertit sans bloquer (BR-JEU-027) ; un jeton de serveur dit où le créer, affiche l'adresse en entier et propose de l'ouvrir (BR-FICHE-059).

- Props : `question` (`FicheQuestion`), `modelValue` (la réponse), `error` (le défaut déjà en phrase), `disabled`
- Événements et slots : `update:modelValue`, `blur`, `link` (ouvrir l'adresse où créer le jeton)
- Notes : Règles pures : `fiche/answers.ts`, textes `fiche/messages.ts`. Tests : `pages/InstallFromFiche.test.ts`.
