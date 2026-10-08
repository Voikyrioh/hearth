# FormDialog

Molécule · `apps/desktop/src/components/molecules/FormDialog.vue`

Fenêtre de formulaire : `<dialog>` natif ouvert par `showModal()` (piège à focus, fond inerte, Échap), téléportée dans `body`, 480 px (`--form-dialog-width`). Le premier champ reçoit le focus (pour les actes d'administration, c'est `AdminActDialog` qui place le curseur, voir sa fiche) ; le focus revient à l'élément d'ouverture. Pendant l'envoi (`busy`) les champs sont figés (`<fieldset disabled>`) et la fenêtre ne se ferme pas ; l'erreur s'affiche DANS la fenêtre qui reste ouverte. « Entrée » envoie seulement si `canSubmit`.

- Props : `open`, `title`, `submitLabel`, `canSubmit`, `busy`, `error`, `destructive`, `cancelLabel`
- Événements et slots : Événements `submit`, `cancel` ; slot par défaut (les champs)
- Notes : Le parent vide ses mots de passe après chaque envoi. Utilisée par `CreateAccountDialog`, `PasswordDialog`, `OwnAccountCard`. Test : `accounts.test.ts`.
