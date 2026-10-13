---
id: BR-GRANT-014
domaine: GRANT
titre: Sous un bouton de service désactivé faute du droit : la raison, et la commande (administrateur) ou « Demande à la personne… » (lecture seule)
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-014), HRT-78
maj: 2026-10-13
---

# BR-GRANT-014 — Sous un bouton de service désactivé faute du droit : la raison, et la commande (administrateur) ou « Demande à la personne… » (lecture seule)

## Règle
`ServiceRightReason` écrit sous le bouton « Ton compte ne peut pas gérer les services de ce serveur. » ; un administrateur voit en plus la phrase d'introduction et la commande à copier pour son propre compte ; un compte en lecture seule voit « Demande à la personne qui administre ce serveur. ». La raison est liée au bouton par `aria-describedby` (`HButton describedBy`). Une autre raison (moteur, lien) garde la sienne ; le pilotage désactivé l'emporte (BR-GRANT-015). Aucun bouton d'action de service n'existe encore : la raison est dite une fois sous la carte du moteur.

## Application (code)
- `apps/desktop/src/services/rights.ts::rightsGate`
- `components/molecules/ServiceRightReason.vue`, `pages/Services.vue`

## Vérification
- `services/rights.test.ts`, `components/molecules/serviceRights.test.ts`, `pages/ServicesRights.test.ts`, `e2e/services-rights.spec.ts`

## Règles liées
BR-GRANT-011, BR-GRANT-015, BR-GRANT-016

## Historique
- 2026-10-13 — création (HRT-78, moitié client).
