# AdminActPanel

Organisme · `apps/desktop/src/components/organisms/AdminActPanel.vue`

Le corps de la confirmation par mot de passe d'un acte d'administration, sans fenêtre (HRT-245, BR-TRUST-036, 042, 043, BR-FICHE-026) : les champs de l'acte (slot par défaut), l'état de la confirmation lu par l'agent (agent d'avant, poste sans clé, élévation de 5 minutes, état de confiance) et « Votre mot de passe » (`ReauthField`) quand il est demandé. Il n'a aucune logique : tout est dans `useAdminAct`, que la fenêtre (`AdminActDialog`) et la page « Vérifiez avant de lancer » appellent chacune pour obtenir le même état. Le champ est vidé après chaque envoi, le mot de passe ne traverse que `perform`.

- Props : `act` (le retour de `useAdminAct`), `serverId`, `passwordLabel?`
- Événements et slots : `leave` (« Aller aux réglages » est pressé : l'enveloppe se ferme) ; slot par défaut (champs de l'acte), slot `after`
- Notes : le premier champ de l'acte est cherché dans le `<fieldset>` parent : la page pose le panneau dans un `fieldset`. L'hôte décide de son bouton : envoyer seulement si `act.canSend`. Tests : `components/organisms/adminActPanel.test.ts` (la même matrice d'états jouée sur la fenêtre et sur le panneau), `adminAct.test.ts`.
