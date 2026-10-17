# OperationCard

Molécule · `apps/desktop/src/components/molecules/OperationCard.vue`

La carte d'une opération de service (HRT-84, BR-SVC-029) : titre (« Déploiement de {nom} »), étape, progression (jauge `HMeter` et chiffres), « Lancée par {compte}. » et, si l'opération attend une réponse, le bouton « Répondre » (« Vérifier » pour un état à vérifier). Reconstruite depuis l'état du serveur, sans fenêtre qui l'a lancée. Sans le droit, le bouton est désactivé avec sa raison écrite dessous. Aucun bouton d'annulation.

- Props : `operation`, `name`, `detailed` (le compte a le droit : progression de l'effacement visible), `by`, `canDecide`
- Événements : `decide` (l'opération)
- Notes : `data-operation`, `data-operation-decide`. Textes dans `serviceOps/messages.ts`. Tests : `pages/ServicesOperations.test.ts`.
