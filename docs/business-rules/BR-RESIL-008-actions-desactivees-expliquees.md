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
Toute action qui a besoin du serveur est désactivée tant que le lien du serveur courant n'est pas « Connecté » (ou que le rôle ne suffit pas, avec `{ role: 'admin' }`). Elle reste focalisable (`aria-disabled`), ignore le clic avant tout gestionnaire et porte une infobulle : « Indisponible tant que le serveur est hors ligne. », « Indisponible pendant la reconnexion au serveur. », « Indisponible : ta session a expiré. », « Indisponible : ton compte n'est plus accessible. », « Réservé aux administrateurs. ». Une seule implémentation : la directive `v-needs-link`. Elle se réactive seule au retour du lien.

## Application (code)
- `apps/desktop/src/directives/needsLink.ts::vNeedsLink` ; textes `fr.ts` groupe `needs` ; usage `apps/desktop/src/pages/Accounts.vue`, `Audit.vue`.

## Vérification
- Tests : `needsLink.test.ts` (4 états, clic bloqué, focus conservé, rôle, serveur courant seul, restitution du `title`), `shell.test.ts::disables the header action of Comptes`, `e2e/shell.spec.ts`.

## Cas limites
- Pas de serveur courant : « Indisponible : aucun serveur sélectionné. ».
- Le rôle est testé avant l'état du lien (un compte en lecture seule voit « Réservé aux administrateurs. »).
- Le blocage côté interface n'est qu'un confort : l'agent refuse de toute façon (BR-ACCT-014).

## Règles liées
- BR-RESIL-001, BR-RESIL-019, BR-ACCT-013, BR-ACCT-014.

## Historique
- 2026-10-05 — création (HRT-09, session 2026-10-04-hearth-creation). Portée par l'interface ; le calcul des états est dans `hearth-link` (ADR-0007).
