# PortMappingRow

Molécule · `apps/desktop/src/components/molecules/PortMappingRow.vue`

Une correspondance de ports du formulaire de déploiement (HRT-69, BR-DEPLOY-007 à 009) : port du serveur, port du service, protocole (TCP ou UDP), « Retirer la correspondance de ports {n} ». Sous la rangée, un message par priorité : l'erreur de forme ou du serveur d'abord, puis l'avertissement (port sous 1024, port réservé par un service arrêté avec l'étiquette « Vous devrez le confirmer au lancement »). Un port refusé propose le port libre que l'AGENT a calculé, en un bouton « Utiliser le port {port} » : la rangée ne recalcule aucun conflit. Un petit indicateur dit que la réponse du serveur n'est pas encore arrivée.

- Props : `row` (`PortRow`), `hostError`, `containerError`, `verdict` (`PortVerdict`), `privileged`, `waiting`, `first`, `position`
- Événements : `update:host`, `update:container`, `update:protocol`, `suggest`, `remove`, `blur-host`, `blur-container`
- Notes : posée par `PortMappingRows`. Tests : `molecules/deployRows.test.ts`.
