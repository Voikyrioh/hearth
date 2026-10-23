# DeployButton

Molécule · `apps/desktop/src/components/molecules/DeployButton.vue`

Le bouton « Déployer un service » de la section Services et de son état vide (HRT-69, BR-DEPLOY-001, 027, BR-GRANT-014). Il reçoit la « porte » calculée par `deploy/availability.ts::deployDoor` : absent (`hidden`) quand le moteur n'est pas utilisable, visible et inactif (`disabled`) faute du droit ou de moteur prêt, actif (`open`). Faute du droit, la raison est dite une seule fois par `ServiceRightReason` (le bouton lui est lié par `aria-describedby`) ; faute de moteur prêt elle est écrite sous le bouton, jamais en infobulle seule ; lien coupé : inactif avec l'explication du lien. Seul bouton principal de la section.

- Props : `door` (`DeployDoor`), `rightsReasonId` (l'`id` du texte de `ServiceRightReason`)
- Événements : `click`
- Notes : posé par `pages/Services.vue` (barre au-dessus de la liste, ou action de `EmptyState`). Tests : `pages/DeployService.test.ts` (le bouton de la section Services), `deploy/deploy.test.ts` (la porte).
