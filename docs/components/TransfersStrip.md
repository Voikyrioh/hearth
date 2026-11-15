# TransfersStrip

Organisme · `apps/desktop/src/components/organisms/TransfersStrip.vue`

Le panneau « Transferts » (HRT-96, BR-FILES-019, BR-MOD-015), monté par le gabarit d'un serveur (`ServerLayout.vue`) au-dessus de la page : il retrouve un envoi de fichier quand on a quitté l'écran qui l'a lancé. Une ligne par envoi : le nom du fichier choisi, l'état en une phrase, une jauge fine et « 420 Mio sur 600 Mio » (octets durables sur le serveur), puis les gestes qui ont un sens : « Reprendre l'envoi » d'un envoi interrompu (jamais automatique), « Annuler » d'un envoi ouvert (la partie reçue est retirée du serveur), « Retirer de la liste » d'un envoi fini. Au plus trois lignes puis « +N autres » qui déplie. Absent sans envoi. Région nommée « Transferts de fichiers » ; la fin ou la coupure d'un envoi est annoncée poliment après 500 ms (zone `.sr-only`), jamais la progression chiffrée. Sous 960 px la jauge passe sous le libellé. Colonnes : le jeton `--op-columns` de la bande des opérations.

- Props : `transfers` (relus par `stores/transfers.ts`)
- Événements : `resume`, `cancel`, `dismiss`
- Notes : aucun contenu de fichier ici, des nombres et le nom choisi ; un manque de place dit ce qui manque et ce qui resterait (BR-FILES-013) et laisse l'envoi reprenable. Textes : `i18n/fr.ts` (`transfers.*`), règles pures `transfers/messages.ts`. Tests : `components/organisms/transfersStrip.test.ts`, `link/simulated-transfers.test.ts`, `e2e/hrt96-transfers.spec.ts`.
