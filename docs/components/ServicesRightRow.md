# ServicesRightRow

Molécule · `apps/desktop/src/components/molecules/ServicesRightRow.vue`

La ligne « Gestion des services » du détail d'un compte (HRT-78, BR-GRANT-013) : « Autorisée » ou « Non autorisée », l'aide « Ce droit se donne et se retire depuis le serveur, pas depuis cet écran. » et `CommandBlock` « Commande à lancer sur le serveur » (`--grant` si non autorisée, `--revoke` si autorisée) avec « Copier la commande » / « Copiée ». LECTURE SEULE.

- Props : `username`, `manages`, `known` (l'agent annonce `service-rights`), `own` (HRT-135 : c'est ton compte ; avant la commande `--revoke`, un avertissement dit ce que tu perds)
- Notes : la commande n'est construite que si `isAccountName(username)` (`accounts/name.ts`, la même validation que l'aide du moteur) ; agent ancien : la valeur dit de mettre le serveur à jour, aucune commande. Test : `serviceRights.test.ts`.
- HRT-135 : la ligne commence par « Ce droit permet de démarrer, arrêter, redémarrer et déployer des services sur ce serveur. » (BR-GRANT-003). La commande est précédée de `sudo` (BR-GRANT-008 : root seulement ; même écriture que l'aide du moteur).
