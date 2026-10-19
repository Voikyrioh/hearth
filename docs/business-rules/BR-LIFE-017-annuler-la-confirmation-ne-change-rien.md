---
id: BR-LIFE-017
domaine: LIFE
titre: Annuler la confirmation ne change rien : rien n'est envoyé ni inscrit
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-017), HRT-65
maj: 2026-10-19
---

# BR-LIFE-017 — Annuler la confirmation ne change rien : rien n'est envoyé ni inscrit

## Règle
Fermer la fenêtre de confirmation (Annuler, Échap, clic hors fenêtre) n'envoie aucune commande et n'écrit rien au journal d'activité ; le service garde son état. Démarrer n'ouvre la fenêtre que si le socle demande un mot de passe : sinon l'ordre part au clic (l'élévation de cinq minutes couvre l'acte).

## Application (code)
- `apps/desktop/src/composables/useServiceLifecycle.ts::{request, cancel}`
- `apps/desktop/src/pages/Services.vue::closeLifecycle`

## Vérification
- `apps/desktop/src/pages/ServicesLifecycle.test.ts (annuler ne change rien : aucun geste, aucun appel)`

## Règles liées
- BR-LIFE-007, BR-LIFE-016

## Historique
- 2026-10-19 — création (HRT-65 : démarrer, arrêter, redémarrer, côté client).
