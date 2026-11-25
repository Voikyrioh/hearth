# ModRow

Molécule · `apps/desktop/src/components/molecules/ModRow.vue`

Une ligne de la liste des mods (HRT-261, HRT-262, maquette Main) : nom et fichier, version, moteur, version de Minecraft visée, pastille d'état (« Compatible », « Non vérifié », « À compléter », « Incompatible », « Installé deux fois », « Non reconnu », « Désactivé », « Retiré ») avec sa phrase sous le nom, origine en quatre valeurs, interrupteur « Activé » (`role="switch"`) et « Retirer » / « Remettre » / « Annuler ». Un geste préparé dit ce qu'il fera au prochain redémarrage (interrupteur en pointillé). Un mod du pack Modrinth et un fichier au nom illisible ont l'interrupteur et « Retirer » inactifs avec leur raison ; sans le droit, la raison est celle du détail (`describedBy`).

- Props : `item`, `game`, `view`, `gate`, `rightsReasonId`, `pending`, `note`, `linkDown`
- Événements : `toggle`, `retire`, `restore`, `undo`
- Notes : aucun geste ne part d'ici ; la ligne émet, le lot est tenu par `useServiceMods`.

Noms de fichier lisibles en entier (retour à la ligne, jamais d'ellipse) ; `wide` et `originColumn` retirent les cellules des colonnes repliées par l'onglet.
