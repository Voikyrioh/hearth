# ConfirmDialog

Molécule · `apps/desktop/src/components/molecules/ConfirmDialog.vue`

Confirmation d'une action : élément `<dialog>` natif ouvert par `showModal()` (piège à focus, fond inerte et Échap par le navigateur), téléporté dans `body`, focus initial sur « Annuler » (bouton par défaut sûr), focus rendu à l'élément qui l'avait ouvert à la fermeture. Bouton destructeur plein seulement ici.

- Props : `open`, `title`, `message`, `confirmLabel`, `cancelLabel`, `destructive`
- Événements et slots : événements `confirm`, `cancel`
- Notes : Seule modale de l'application hors empreinte changée ; jamais utilisée pour une perte de lien (BR-RESIL-011). Tests : `molecules.test.ts`.
