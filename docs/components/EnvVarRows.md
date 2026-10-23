# EnvVarRows

Organisme · `apps/desktop/src/components/organisms/EnvVarRows.vue`

La section « Variables d'environnement » du formulaire de déploiement (HRT-69, BR-DEPLOY-002, 006) : de 0 à 100 rangées (`EnvVarRow`), compteur « n sur 100 », avertissement permanent sur la visibilité des valeurs, « Ajouter une variable ». À 100 le bouton reste visible et inactif, « Limite atteinte » écrit à côté. Les défauts de forme ne se montrent qu'une fois le champ quitté (`shown`). La section n'écrit nulle part les noms ni les valeurs.

- Props : `rows`, `issues`, `shown`
- Événements : `add`, `remove`, `update`, `touch`
- Notes : posée par `DeployForm`. Tests : `pages/DeployService.test.ts`.
