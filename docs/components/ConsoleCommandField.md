# ConsoleCommandField

Molécule · `apps/desktop/src/components/molecules/ConsoleCommandField.vue`

Le champ « Commande » (HRT-269, BR-CONSOLE-003, 005, 006, 008, maquettes Console et ConsoleEtats) : une ligne de 1 à 256 caractères (retour à la ligne refusé avant l'envoi), « Envoyer », et toujours une phrase qui dit pourquoi le champ ou le bouton est inactif (arrêté avec « Démarrer », qui démarre, opération en cours avec « Voir le suivi », lecture seule, trop de commandes avec le délai). Entrée envoie.

- Props : modelValue, block, issue, judged, canSend, sending, maxChars, canStart
- Événements : update:modelValue, send, start, follow
- Notes : tests `pages/ServicesConsole.test.ts`, e2e `e2e/console.spec.ts`.
