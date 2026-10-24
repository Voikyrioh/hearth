# ProgressSteps

Molécule · `apps/desktop/src/components/molecules/ProgressSteps.vue`

Les étapes d'une opération, une à la fois (HRT-69, BR-DEPLOY-016) : faite = coche, en cours = point braise (`aria-current="step"`), à venir = cercle vide ; l'état est aussi écrit pour les lecteurs d'écran. L'étape qui progresse porte une barre `HMeter` (jauge bornée de 0 à 100 %, ou barre indéterminée quand la progression est inconnue) et son texte (« 40 % · 400 Mo sur 1 Go »). Rien n'est calculé ici : l'agent donne l'étape et la progression, l'appelant les libellés.

- Props : `steps` (`ProgressStep[]` : `key`, `label`, `state`, `bar?`), `label`
- Notes : posée par `DeployProgressDialog`. Même langage que `AgentUpdateSteps`. Tests : `molecules/deployRows.test.ts`.
