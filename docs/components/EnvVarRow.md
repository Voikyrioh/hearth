# EnvVarRow

Molécule · `apps/desktop/src/components/molecules/EnvVarRow.vue`

Une variable d'environnement du formulaire de déploiement (HRT-69, BR-DEPLOY-006) : nom, valeur, bouton pour montrer ou masquer la valeur (`aria-pressed`), bouton « Retirer la variable {nom} ». La valeur est masquée d'office quand le nom évoque un secret (PASSWORD, PASS, SECRET, TOKEN, KEY, CREDENTIAL) ; l'utilisateur peut montrer ou masquer n'importe quelle valeur. Le nom et la valeur sont des secrets : jamais dans un titre, une infobulle ou une trace. Les libellés ne sont visibles que sur la première rangée (`first`), les suivantes les gardent pour les lecteurs d'écran. La rangée ne juge rien : les défauts arrivent en phrases.

- Props : `row` (`EnvRow`), `nameError`, `valueError`, `first`
- Événements : `update:name`, `update:value`, `toggle`, `remove`, `blur-name`, `blur-value`
- Notes : posée par `EnvVarRows`. Tests : `molecules/deployRows.test.ts`.
