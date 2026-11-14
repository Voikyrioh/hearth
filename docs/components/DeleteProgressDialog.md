# DeleteProgressDialog

Organisme · `apps/desktop/src/components/organisms/DeleteProgressDialog.vue`

Le suivi d'une suppression (HRT-86, BR-DEL-013 à 016, 020, 023, 030, 031) : la fenêtre de confirmation devient cette vue (`FormDialog`, slot `actions`). Les étapes dans l'ordre du plan (`ProgressSteps` : arrêt du service seulement s'il tournait, suppression du service, effacement des données seulement sur la case, avec le nombre d'éléments effacés), « Annuler la suppression » (acte confirmé par `AdminActDialog`, genre `operation_cancel`, couvert par l'élévation ; inactif avec « Il est trop tard pour annuler » dès que le conteneur est supprimé) et « Fermer » (la suppression continue, l'effacement reste dans la bande des opérations). Les issues se disent ici par leur cause : service supprimé (données conservées dans {chemins}, ou données effacées), supprimé avec des données qui n'ont pas pu partir (la liste des dossiers concernés, jamais rejouée), arrêt forcé, pas supprimé (le service reste entier), annulée, résultat inconnu (lien coupé : rien n'est relancé).

- Props : `open`, `serverId`, `launch`, `operation`, `ended`, `performCancel`
- Événements : `close`
- Notes : posée par `pages/Services.vue`. Tests : `pages/ServicesDelete.test.ts`, `e2e/hrt86-delete.spec.ts`.
