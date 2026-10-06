---
id: BR-AUDIT-020
domaine: AUDIT
titre: Rechargement manuel quand le lien est coupé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-020), HRT-05
maj: 2026-10-06
---

# BR-AUDIT-020 — Rechargement manuel quand le lien est coupé

## Règle
Côté interface : aucune relance automatique ; un bouton de rechargement manuel. Côté agent : `GET /audit` est sans état et idempotent ; recharger rend la page la plus récente.

## Application (code)
- Sans objet côté agent.
- `apps/desktop/src/stores/audit.ts::useAuditStore::reloadManually`
- `apps/desktop/src/pages/Audit.vue`

## Interface
Aucune relecture automatique pendant la coupure (les lectures hors « Connecté » échouent sans rien envoyer). « Rechargement manuel » : « Tentative de reconnexion », puis relecture au retour du lien ; si le lien ne revient pas sous 5 s : « Serveur toujours injoignable » et le bouton reste.

## Vérification
- `apps/desktop/src/stores/audit.test.ts` et `apps/desktop/src/pages/audit.test.ts` (« Rechargement manuel »).

## Cas limites
- Aucun cas limite propre à l'agent.

## Règles liées
- BR-AUDIT-011.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » et pointeurs du client (HRT-14, session 2026-10-04-hearth-creation).
