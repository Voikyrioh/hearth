# SecurityAlertCard

Organisme · `apps/desktop/src/components/organisms/SecurityAlertCard.vue`

Carte « Ce qui se passe » de la page Sécurité (HRT-39, C36) : ce que « Plus d'infos » promet. Dit si ton identifiant est visé et depuis quand, combien d'AUTRES comptes le sont (administrateur seulement, jamais leurs noms, lien vers le journal d'activité) et quoi faire. N'a pas de bouton d'activation : celui de la carte « Mode attaque » est le seul de la page. Visible seulement tant que l'alerte dure.

- Props : `alert`, `role`, `serverId`, `modeOn`
- Événements et slots : aucun
- Notes : FIX-01M4DJZB43SA08NE46DGEZK213. Tests : `pages/SecurityMode.test.ts`, `e2e/hrt39-security.spec.ts`.
- FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB : `modeUnknown` remplace « déjà actif » et « active-le » par la phrase neutre « Ce client ne sait pas lire l'état de protection de ce serveur… ».
