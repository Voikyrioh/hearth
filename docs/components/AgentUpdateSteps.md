# AgentUpdateSteps

Molécule · `apps/desktop/src/components/molecules/AgentUpdateSteps.vue`

Les cinq étapes d'une mise à jour de l'agent, une à la fois (BR-UPDATE-013) : Téléchargement (avec « : 35 % » et une barre `HMeter`), Vérification, Installation, Redémarrage, Contrôle. Faite = coche `--ok`, en cours = point braise, à venir = cercle vide ; l'état est aussi dit en texte pour les lecteurs d'écran. Le pourcentage vient de l'agent, il est seulement borné à 0 à 100 pour la barre.

- Props : `step` (l'étape en cours), `percent` (téléchargement, ou `null`)
- Événements et slots : aucun
- Notes : `data-step` et `data-state` sur chaque ligne (tests). Tests : `agentUpdate.test.ts`.
