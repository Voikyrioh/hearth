# BackupModsCompare

Molécule · `apps/desktop/src/components/molecules/BackupModsCompare.vue`

La comparaison d'une sauvegarde avec les mods actuels (HRT-294, BR-MOD-036) : les changements regroupés par genre (ajoutés depuis, retirés depuis, version changée, activés depuis, désactivés depuis) ou « Les mods actuels sont les mêmes que dans cette sauvegarde. ». Calculée par l'agent, jamais par le client ; partielle tant que la liste actuelle n'est pas lue en entier (« Comparaison partielle : {n} mods ne sont pas encore lus. », relue seule).

- Props : `compare`
- Notes : tests `pages/ServicesBackupsMods.test.ts`.
