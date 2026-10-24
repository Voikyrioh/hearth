# AgreementDialog

Organisme · `apps/desktop/src/components/organisms/AgreementDialog.vue`

La fenêtre du contrat de licence d'une fiche (HRT-102, BR-FICHE-058, 026). Hearth n'accepte jamais un contrat à la place de l'utilisateur : la fenêtre s'ouvre à « Installer le serveur de jeu », avant le mot de passe, même après « Installer avec les valeurs proposées ». Titre du contrat, phrase de la fiche, « Lisez le contrat avant de répondre. », « Lire le contrat » (le navigateur de l'utilisateur ; la page n'est jamais chargée dans Hearth), puis « Je refuse » et « J'accepte » : deux boutons secondaires de même poids, le focus étant sur « Lire le contrat ». Échap vaut « Je refuse ».

- Props : `open`, `agreement`, `openFailed`
- Événements et slots : `read`, `accept`, `refuse`
- Notes : Posé par `FicheSummaryDialog`, qui l'ouvre par `AdminActDialog.beforeSubmit`. Tests : `pages/InstallFromFiche.test.ts`.
