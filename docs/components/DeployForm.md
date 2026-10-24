# DeployForm

Organisme · `apps/desktop/src/components/organisms/DeployForm.vue`

Le formulaire « Déployer un service » (HRT-69, BR-DEPLOY-002 à 009, 014, 031, 033), en colonne unique de 880 px : Général (nom, image, version), Variables d'environnement, Ports, Commande, Ressources (`ResourceSliders`, bornes lues sur la machine, jamais « sans limite »), puis un pied collant : à gauche la raison pour laquelle « Déployer » est inactif, à droite le bouton. Les dossiers de données arrivent avec HRT-70. Les défauts de forme viennent de `deploy/rules.ts`, les constats de conflit de l'agent par `composables/useDeployForm.ts` ; tout est dit indicatif. Le focus initial est sur « Nom du service ». Un refus du serveur au dernier envoi s'affiche en tête (`refusal`) et la saisie reste.

- Props : `form` (`DeployFormState`), `refusal`, `reason` (`{ text, fixable }`), `busy`
- Événements : `submit`. Expose `focusField(field)`.
- Notes : posé par `pages/DeployService.vue`. Tests : `pages/DeployService.test.ts`, `e2e/hrt69-deploy.spec.ts`.
- Revue UX du 2026-10-24 (constat 22) : le champ Image montre l'exemple court « nginx » et explique le nom complet en aide dessous (« registre/organisation/nom », registre et organisation facultatifs).
