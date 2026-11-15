# PublicAddressDialog

Organisme · `apps/desktop/src/components/organisms/PublicAddressDialog.vue`

La fenêtre « Adresse pour vos joueurs » (HRT-215, BR-JEU-022) : un champ facultatif (nom de domaine ou adresse IP, sans protocole ni port, 253 caractères au plus) et la confirmation du socle dans UNE fenêtre (`AdminActDialog`, acte `service_update`, jamais couvert par l'élévation : le mot de passe est demandé à chaque fois). Le champ vide efface l'adresse. Le client refuse avant d'envoyer ; l'agent juge ensuite la forme.

- Props : `open`, `serverId`, `current`, `perform`
- Événements : `close`
- Notes : posée par `GameCards`. Tests : `pages/ServicesGame.test.ts`.
