---
id: BR-RESIL-005
domaine: RESIL
titre: Reconnexion automatique sans fin, et « Réessayer maintenant » relance tout de suite
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-005), HRT-09
maj: 2026-10-05
---

# BR-RESIL-005 — Reconnexion automatique sans fin, et « Réessayer maintenant » relance tout de suite

## Règle
Le client se reconnecte seul, sans fin, avec des intervalles de plus en plus espacés jusqu'à 30 secondes ; le clic sur « Réessayer maintenant » relance une tentative immédiate. **Part de l'interface** : le bouton du bandeau hors ligne appelle `LinkBridge.retryNow(serverId)` ; l'état passe à « Reconnexion… » puis « Connecté » ou revient à « Hors ligne », sans modale. **Part hors interface** : le calcul des intervalles et la tentative elle-même vivent dans `hearth-link` (ADR-0007, machine à états du lien), pas dans le client web.

## Application (code)
- `apps/desktop/src/link/bridge.ts::LinkBridge.retryNow` (contrat) ; `apps/desktop/src/stores/link.ts::retryNow` ; `apps/desktop/src/components/organisms/OfflineBanner.vue` (bouton, événement `retry`) ; `apps/desktop/src/layouts/ServerLayout.vue` (branche `retry` au store).
- Implémentation simulée : `apps/desktop/src/link/simulated.ts::SimulatedLinkBridge.retryNow`. Le pont réel et les intervalles : ticket `hearth-link`.

## Vérification
- Tests : `link-stores.test.ts::goes reconnecting on « Réessayer maintenant », then back to connected`, `::asks the bridge to retry now`, `shell.test.ts::« Réessayer maintenant » asks the bridge to retry that server`, `shell.test.ts::a rejecting action (retryNow) leaves the shell and the page intact`, `e2e/shell.spec.ts` (« connecté, reconnexion, hors ligne puis retour »).

## Cas limites
- Si `retryNow` rejette (liaison en panne) : notification discrète et journal, la coquille et la page restent affichées (BR-RESIL-011).

## Règles liées
- BR-RESIL-003, BR-RESIL-004, BR-RESIL-011.

## Historique
- 2026-10-05 — création (HRT-09, revue Stephen round 1 : références sans fiche). Portée par l'interface pour ce qui la concerne ; la reconnexion elle-même est dans `hearth-link` (ADR-0007).
