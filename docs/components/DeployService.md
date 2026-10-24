# DeployService

Page · `apps/desktop/src/pages/DeployService.vue`

La page « Déployer un service » (HRT-69) : le formulaire en page (`DeployForm`), puis le récapitulatif (`DeploySummaryDialog`) et la progression (`DeployProgressDialog`) en fenêtres, « Abandonner votre saisie ? » (`ConfirmDialog`) et, pour un service qui boucle, la fenêtre d'arrêt (`ServiceLifecycleDialog`). Route `/servers/:id/services/deployer` (`services-deploy`), gardée par `router/deployGuard.ts` : un compte sans le droit de gérer les services et un moteur inutilisable renvoient vers la liste ; un moteur qui n'est pas prêt laisse le formulaire s'ouvrir et l'explique (`EnginePanel`). « Fermer » (en-tête, Échap hors d'un champ, autre entrée du menu) demande de confirmer quand un champ est rempli. La saisie vit en mémoire seulement (`stores/deployForm.ts`) et s'efface en quittant la page.

- Aucune prop. Parcours : `composables/useDeployForm.ts` (saisie, constats), `composables/useDeployFlow.ts` (récapitulatif, envoi, progression).
- Tests : `pages/DeployService.test.ts`, `pages/DeployFlow.test.ts`, `e2e/hrt69-deploy.spec.ts`.
