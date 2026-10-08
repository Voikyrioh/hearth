# AgentUpdateCard

Organisme · `apps/desktop/src/components/organisms/AgentUpdateCard.vue`

Carte « État du serveur : {nom} » des réglages (une par serveur, HRT-17, ADR-0021) : versions « Client {version} · Agent {version} » (BR-UPDATE-022, 025), mention « Mise à jour disponible » (BR-UPDATE-023), bouton « Mettre à jour l'agent » (administrateur ; retiré pendant la mise à jour, HRT-46 ; désactivé avec « Seul un administrateur peut mettre à jour l'agent » pour le rôle Lecture seule, BR-UPDATE-011), confirmation « Mettre à jour l'agent ? Cette opération redémarrera l'agent brièvement. » (`ConfirmDialog`, boutons « Oui, mettre à jour » / « Annuler »), les étapes une à une avec le pourcentage du téléchargement (`AgentUpdateSteps`, BR-UPDATE-013), la pastille du lien (« Reconnexion… » pendant le redémarrage, sans erreur, BR-UPDATE-014), le résultat (réussi, annulé, échoué : textes de la spécification, BR-UPDATE-015, 017, 019, avec « Compris »), une ligne d'historique pour un résultat de plus de 24 h, l'explication sans bouton en installation gérée, et le message d'incompatibilité de versions (BR-UPDATE-020, 021).

L'interface ne fournit NI adresse, NI signature, NI somme : elle ne transmet que le numéro de la version qu'elle montre (`updateAgent(serverId, version)`) ; la coquille tient la cible de sa propre lecture du flux de versions. Le bouton exige le lien de CE serveur (`needsLink` avec `server`) ; l'agent reste l'arbitre du rôle.

- Props : `server`
- Événements et slots : aucun
- Notes : place choisie : colonne de droite des réglages, au-dessus de « Mon compte » (le design y met une carte « État du serveur » par serveur). Store : `agentUpdates`. Tests : `agentUpdate.test.ts`, `stores/agentUpdates.test.ts`, `e2e/agent-update.spec.ts`.

HRT-30 : la confirmation de la mise à jour passe par `AdminActDialog` (kind `agent_update`, jamais couvert par le délai : mot de passe toujours demandé).
