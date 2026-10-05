---
id: BR-RESIL-018
domaine: RESIL
titre: Coupures répétées : une même notification répétée devient un compteur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-018), HRT-09
maj: 2026-10-05
---

# BR-RESIL-018 — Coupures répétées : une même notification répétée devient un compteur

## Règle
Quand la même notification (même type, même texte) est émise plusieurs fois, la pile n'ajoute pas de ligne : un compteur « 5 fois » apparaît sur la ligne existante et sa durée d'affichage est renouvelée. Des textes ou des types différents restent séparés. La file ne dépasse jamais 50 notifications (session longue, BR-RESIL-017). La bibliothèque expose le nombre d'échecs consécutifs (`Status::failed_attempts`) ; l'agrégation est de l'interface.

## Application (code)
- Bibliothèque :
  - `Status::failed_attempts` fourni par `crates/hearth-link/src/domain/state.rs::LinkMachine::status` et par les événements du `LinkManager`.
- Interface :
  - `apps/desktop/src/stores/toasts.ts::push` (déduplication par type et texte, `count`) ; affichage `apps/desktop/src/components/molecules/ToastStack.vue` (`toast__count`).
- Interface : notification à clé (`toasts.ts::push` avec `key`, `dismissKey`) : « {serveur} : Reconnexion échouée {n} fois. » se met à jour sur place à partir de 3 échecs consécutifs (`link.ts::RECONNECT_FAILURE_NOTICE_FROM`) et disparaît au retour du lien. Coquille : l'agrégation des notifications système est dans `presence.rs::NotificationGate` (BR-RESIL-015).

## Vérification
- Interface : `link-stores.test.ts::counts a repeated notification instead of stacking it`, `::does not merge the same text of another kind`, `::dismisses on its own after the lifetime, and a repeat renews it`, `molecules.test.ts::turns a repeated notification into a counter`, `errors.test.ts::bounds the rate`, `e2e/shell.spec.ts`.
- Bibliothèque : voir les tests des règles BR-RESIL-002 à BR-RESIL-005 (compteur d'échecs consécutifs).
- Interface : `apps/desktop/src/stores/offline.test.ts``::compte les échecs dans UNE notification par serveur, qui monte sur place`, `apps/desktop/e2e/offline.spec.ts` (« coupures répétées »).

## Cas limites
- Les notifications masquées par la limite de 3 gardent leur compteur.

## Règles liées
- BR-RESIL-002 à BR-RESIL-005, BR-RESIL-011, BR-RESIL-017.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — création de la partie interface (HRT-09, session 2026-10-04-hearth-creation). Portée par l'interface ; le calcul des états est dans `hearth-link` (ADR-0007).
- 2026-10-05 — fiches HRT-07 et HRT-09 réunies (fusion de main dans feat/HRT-07-link).
- 2026-10-05 : compteur d'échecs de reconnexion, une notification par serveur (HRT-12).
