# ConfirmDialog

Molécule · `apps/desktop/src/components/molecules/ConfirmDialog.vue`

Confirmation d'une action : `alertdialog` modale, piège à focus, Échap = annuler, focus initial sur « Annuler » (bouton par défaut sûr), focus rendu à l'ouverture. Bouton destructeur plein seulement ici.

- Props : `open`, `title`, `message`, `confirmLabel`, `cancelLabel`, `destructive`
- Événements et slots : événements `confirm`, `cancel`
- Notes : Seule modale de l'application hors empreinte changée ; jamais utilisée pour une perte de lien (BR-RESIL-011). Tests : `molecules.test.ts`.
