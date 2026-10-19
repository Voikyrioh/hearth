# OperationDecisionDialog

Organisme · `apps/desktop/src/components/organisms/OperationDecisionDialog.vue`

La fenêtre de décision d'une opération de service (HRT-84) : les choix offerts par l'agent (aucun préchoisi), l'échéance, puis le mot de passe redemandé à chaque réponse (`AdminActDialog`, genre `operation_decide`, jamais couvert par l'élévation). Un refus se dit dans la fenêtre ; un résultat inconnu la ferme, sans rejeu.

- Props : `open`, `serverId`, `tracked`
- Événements : `close`
- Notes : un choix inconnu de ce client n'est jamais proposé. Tests : `pages/ServicesOperations.test.ts`.
