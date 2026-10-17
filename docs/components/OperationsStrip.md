# OperationsStrip

Organisme · `apps/desktop/src/components/organisms/OperationsStrip.vue`

La bande « Opérations en cours » de la section Services (HRT-84, BR-SVC-029), sous la carte du moteur : une ligne par opération (titre, étape, jauge fine, qui l'a lancée, « Voir », et « Répondre » quand une décision est attendue), au plus trois lignes puis « +N autres » qui déplie. Absente sans opération. Région nommée « Opérations en cours », nombre annoncé poliment après 500 ms, aucune progression chiffrée annoncée. Sous 960 px la jauge passe sous le libellé.

- Props : `operations` (suivies par `stores/serviceOps.ts`), `launchedBy`, `allowed` (droit « gérer les services »)
- Événements : `see` (ouvre le détail du service), `decide`
- Notes : sans le droit, un effacement de données est une phrase seule. Tests : `pages/ServicesOperations.test.ts`, `e2e/hrt84-operations.spec.ts`.
