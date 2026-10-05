---
id: BR-RESIL-008
domaine: RESIL
titre: Hors « Connecté », les actions qui exigent le serveur sont désactivées et expliquées
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-008), HRT-09
maj: 2026-10-05
---

# BR-RESIL-008 — Hors « Connecté », les actions qui exigent le serveur sont désactivées et expliquées

## Règle
Toute action qui a besoin du serveur est désactivée tant que le lien du serveur courant n'est pas « Connecté » (ou que le rôle ne suffit pas, avec `{ role: 'admin' }`). Elle reste focalisable (`aria-disabled`), ignore le clic et explique pourquoi dans une infobulle (`HTooltip`, au survol ET au focus clavier, `aria-describedby`) : « Indisponible tant que le serveur est hors ligne. », « Indisponible pendant la reconnexion au serveur. », « Indisponible : ta session a expiré. », « Indisponible : ton compte n'est plus accessible. », « Réservé aux administrateurs. ». Une seule implémentation : la prop `needsLink` de `HButton` et `HToggle`, qui appelle `useNeedsLink`. L'état final est `disabled || busy || raison du lien`, calculé dans le composant : un bouton occupé dont le lien tombe puis revient ne reprend jamais un aspect actif à tort. Il se réactive seul au retour du lien. La bibliothèque refuse d'ailleurs l'envoi hors « Connecté » (`LinkError::NotConnected`, voir BR-RESIL-009) ; la désactivation visuelle est de l'interface.

## Application (code)
- Bibliothèque :
  - Refus d'envoi hors « Connecté » (`LinkError::NotConnected`, voir BR-RESIL-009) ; état fourni par `crates/hearth-link/src/domain/state.rs::LinkMachine::status` et par les événements du `LinkManager`.
- Interface :
  - `apps/desktop/src/composables/useNeedsLink.ts::useNeedsLink` ; `apps/desktop/src/components/atoms/HButton.vue`, `HToggle.vue` (prop `needsLink`) ; `HTooltip.vue` ; textes `fr.ts` groupe `needs` ; usage `apps/desktop/src/pages/Accounts.vue`, `Audit.vue`.

## Vérification
- Interface : `HButton.test.ts::needs-link on HButton and HToggle` (4 états, busy et lien combinés, focus clavier et infobulle, rôle, serveur courant seul, HToggle), `shell.test.ts::disables the header action of Comptes`, `e2e/shell.spec.ts`.
- Bibliothèque : voir BR-RESIL-009 (refus `NotConnected`).

## Cas limites
- Pas de serveur courant : « Indisponible : aucun serveur sélectionné. ».
- Le rôle est testé avant l'état du lien (un compte en lecture seule voit « Réservé aux administrateurs. »).
- Le blocage côté interface n'est qu'un confort : l'agent refuse de toute façon (BR-ACCT-014).

## Règles liées
- BR-RESIL-001, BR-RESIL-002 à BR-RESIL-005, BR-RESIL-009, BR-RESIL-019, BR-ACCT-013, BR-ACCT-014.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — création de la partie interface (HRT-09, session 2026-10-04-hearth-creation). Portée par l'interface ; le calcul des états est dans `hearth-link` (ADR-0007).
- 2026-10-05 — fiches HRT-07 et HRT-09 réunies (fusion de main dans feat/HRT-07-link).
