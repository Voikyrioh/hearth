# ReleaseNotesDialog

Molécule · `apps/desktop/src/components/molecules/ReleaseNotesDialog.vue`

Boîte « Notes de version » : numéro de version et notes en **texte brut** (`white-space: pre-wrap`, jamais interprétées comme du HTML), bouton « Fermer ». `<dialog>` natif ouvert par `showModal()` et téléporté dans `body` (piège à focus, Échap) ; le focus revient à l'élément d'origine. Notes vides : « Aucune note pour cette version. ».

- Props : `open`, `version`, `notes`
- Événements et slots : événement `close`
- Notes : `data-release-notes` sur le texte. Utilisé par `UpdateBanner` et `UpdatePanel`. Tests : `components/organisms/updates.test.ts`.
