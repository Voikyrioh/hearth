---
id: BR-DEPLOY-015
domaine: DEPLOY
titre: Le récapitulatif d'un déploiement tient dans une seule fenêtre, avec la confirmation du socle jamais couverte par l'élévation, et ne s'active que lorsque chaque point d'attention est validé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-015), HRT-69
maj: 2026-10-24
---

# BR-DEPLOY-015 — Le récapitulatif d'un déploiement tient dans une seule fenêtre, avec la confirmation du socle jamais couverte par l'élévation, et ne s'active que lorsque chaque point d'attention est validé

## Règle
« Déployer » ouvre une fenêtre unique « Vérifiez avant de lancer » : nom, image et version (« par défaut » quand « latest » n'est pas saisie), ports (avec qui peut les joindre), nombre de variables et leurs noms (valeurs jamais en clair), commande personnalisée ou non (le texte de la commande n'est pas répété), redémarrage, ressources, puis les **points d'attention** qui s'appliquent, chacun une case : port réservé par un service arrêté (`BR-DEPLOY-008`), manque de place (`BR-DEPLOY-018`). « Déployer le service » reste inactif tant qu'un point n'est pas coché. La consultation est refaite à l'ouverture (le serveur a le dernier mot, `BR-DEPLOY-032`) : un point devenu bloquant ramène au champ sans ouvrir la fenêtre. Le mot de passe est demandé à chaque déploiement, même juste après un autre acte (`service_deploy` n'est jamais couvert par l'élévation de 5 minutes) ; un refus de l'agent faute de place fait apparaître la case même si la consultation ne l'avait pas vue.

## Application (code)
- `apps/desktop/src/components/organisms/DeploySummaryDialog.vue`
- `apps/desktop/src/composables/useDeployFlow.ts::{openSummary, perform}`
- `apps/desktop/src/link/simulated-reauth.ts::simulatedCovers (`service_deploy` : non couvert)`

## Vérification
- `apps/desktop/src/pages/DeployFlow.test.ts (le récapitulatif)`
- `apps/desktop/e2e/hrt69-deploy.spec.ts`

## Cas limites
- Consultation impossible à l'ouverture (lien, moteur) : le récapitulatif s'ouvre sans point d'attention ; le serveur refusera au besoin.
- Le texte de la commande n'est pas répété dans le récapitulatif (il est déjà dans le formulaire, « Modifier » y revient) : choix conservateur, un secret mal placé n'est pas réaffiché.

## Règles liées
BR-DEPLOY-008, BR-DEPLOY-018, BR-DEPLOY-031, BR-DEPLOY-032, BR-GRANT-017

## Historique
- 2026-10-24 — création (HRT-69, déployer une image, côté client).
