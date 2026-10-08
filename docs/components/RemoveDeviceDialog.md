# RemoveDeviceDialog

Organisme · `apps/desktop/src/components/organisms/RemoveDeviceDialog.vue`

Confirmation du retrait d'un poste de confiance (HRT-23, Q16 : un acte d'administration exige le mot de passe ET la clé). `FormDialog` destructif : titre « Retirer {nom} ? », texte de la spécification, ligne « Pour retirer un poste, confirme ton mot de passe. », champ mot de passe. Le mot de passe faux s'affiche sous le champ, les autres refus (poste disparu, serveur occupé, trop d'essais) dans la fenêtre qui reste ouverte. Le champ est vidé après chaque envoi, réussi ou non, et à chaque ouverture. Lien coupé avant la réponse : la fenêtre se ferme, « Le résultat de cette action n'est pas connu. », jamais rejouée. La preuve de la clé de ce PC est faite par la coquille, sans geste de l'utilisateur.

- Props : `open`, `serverId`, `device` (`TrustedDevice | null`)
- Événements et slots : `close`
- Notes : passe par `useDeviceActions` (donc `useServerAction`). Tests : `pages/Security.test.ts`, `e2e/security.spec.ts`.
- HRT-26 : la fenêtre dit aussi « Change aussi ton mot de passe si ce poste n'est plus à toi » (un poste retiré depuis un autre PC peut se reconnaître à sa prochaine connexion par mot de passe).
- HRT-39 : un paragraphe (les deux phrases de la spec) puis le champ ; le conseil « Si ce poste n'est plus à toi » est replié. FIX:01M4ECZJKG6MQ2C65TZP25WSPD.
