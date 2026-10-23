# DeployProgressDialog

Organisme · `apps/desktop/src/components/organisms/DeployProgressDialog.vue`

La progression d'un déploiement (HRT-69, BR-DEPLOY-016, 019, 020, 028) : une fenêtre modale de 560 px qui remplace le récapitulatif au lancement. Les cinq étapes de l'agent dans l'ordre (`ProgressSteps`), la progression de la récupération (pourcentage et octets, sinon barre indéterminée), « Annuler le déploiement » (acte confirmé par `AdminActDialog`, genre `operation_cancel`, couvert par l'élévation ; inactif avec « Trop tard pour annuler » dès que la création a commencé) et « Continuer en arrière-plan » (focus initial, Échap). Les issues se disent ici avec leur cause : échec où rien n'a été créé (« Modifier »), service conservé arrêté juste après son démarrage (« Voir les journaux », « Arrêter » s'il boucle, « Fermer »), résultat inconnu (seul « en arrière-plan »), résultat à vérifier. Succès et annulation sont dits par une notification, la page ferme la fenêtre.

- Props : `open`, `serverId`, `name`, `operation`, `ended`, `unknown`, `service`, `performCancel`
- Événements : `background`, `modify`, `logs`, `stop`, `close`
- Notes : posé par `pages/DeployService.vue`. Tests : `pages/DeployFlow.test.ts`, `e2e/hrt69-deploy.spec.ts`.
