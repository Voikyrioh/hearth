# AttackModeDialog

Organisme · `apps/desktop/src/components/organisms/AttackModeDialog.vue`

Confirmation de l'activation ou de la désactivation du mode attaque (HRT-26, Q14 point 3 et Q16 : un acte d'administration exige le mot de passe ET la clé). `FormDialog` non destructif : titre « Activer le mode attaque ? » / « Désactiver le mode attaque ? », ce que le mode change, ligne « Pour changer le mode attaque, confirme ton mot de passe. », champ mot de passe. Mot de passe faux : sous le champ ; autres refus (trop d'essais, serveur occupé) dans la fenêtre qui reste ouverte. Le champ est vidé après chaque envoi, réussi ou non, et à chaque ouverture. Lien coupé avant la réponse : la fenêtre se ferme, « Le résultat de cette action n'est pas connu. », jamais rejouée. La preuve de la clé de ce PC est faite par la coquille.

- Props : `open`, `serverId`, `active` (le geste demandé)
- Événements et slots : `close`
- Notes : rendue par `ServerLayout` (une seule, état `security.dialog` du store). Passe par `useAttackModeActions` (donc `useServerAction`). Tests : `pages/SecurityMode.test.ts`.
