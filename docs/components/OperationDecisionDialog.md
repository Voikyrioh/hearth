# OperationDecisionDialog

Organisme · `apps/desktop/src/components/organisms/OperationDecisionDialog.vue`

La fenêtre de décision d'une opération de service (HRT-84) : les choix offerts par l'agent (aucun préchoisi), l'échéance, puis le mot de passe redemandé à chaque réponse (`AdminActDialog`, genre `operation_decide`, jamais couvert par l'élévation). Un refus se dit dans la fenêtre ; un résultat inconnu la ferme, sans rejeu.

- Props : `open`, `serverId`, `tracked`
- Événements : `close`
- Notes : un choix inconnu de ce client n'est jamais proposé. Tests : `pages/ServicesOperations.test.ts`.

- HRT-104 : pour l'arrêt ou le redémarrage d'un serveur de jeu dont la confirmation d'enregistrement manque (BR-JEU-006, BR-JEU-007), la fenêtre a sa propre mise en page : `Callout` de la phrase de la règle, « Rien n'a été arrêté : le service tourne toujours. », « Annuler : le service reste en marche » présélectionné et en premier, « Arrêter quand même » (envoi en rouge), « Décider plus tard » pour quitter sans répondre. Prop `gameName` (le nom du jeu quand le geste vient de cette page). Les autres décisions sont inchangées.
