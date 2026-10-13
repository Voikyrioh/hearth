---
id: BR-ENGINE-015
domaine: ENGINE
titre: Lien coupé : la dernière information du moteur reste, périmée et datée, sans erreur inventée
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-015), HRT-62
maj: 2026-10-13
---

# BR-ENGINE-015 — Lien coupé : la dernière information du moteur reste, périmée et datée, sans erreur inventée

## Règle
Quand le lien avec le serveur n'est pas « Connecté », la dernière information sur le moteur reste affichée, marquée périmée avec son âge (`BR-RESIL-007`). Hearth n'invente jamais une erreur du moteur à partir d'une coupure du lien : « Ne répond pas » ne s'affiche que si la liaison est établie et que le moteur ne répond pas. Lien coupé sans rien reçu : ni liste ni erreur.

## Application (code)
- `apps/desktop/src/pages/Services.vue` : le gabarit du serveur (`StaleSurface`) grise et date ; la mention « Liste du moteur d'il y a… » n'apparaît que lien bon.
- `apps/desktop/src/stores/services.ts::apply` : la liaison n'émet plus d'événement hors « Connecté », le magasin garde le dernier état.
- HORS DOMAIN : règle d'affichage portée par l'interface ; l'état « Ne répond pas » vient de l'agent (`BR-ENGINE-009`), jamais d'une coupure du lien.

## Vérification
- `apps/desktop/src/pages/Services.test.ts` : « lien coupé sans rien reçu : ni liste ni erreur du moteur inventée (BR-ENGINE-015) ».

## Règles liées
- BR-ENGINE-009, BR-RESIL-007, BR-SVC-017

## Historique
- 2026-10-13 — création (HRT-62, section Services du client, lecture seule).
