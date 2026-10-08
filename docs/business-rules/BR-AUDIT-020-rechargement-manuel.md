---
id: BR-AUDIT-020
domaine: AUDIT
titre: Rechargement manuel quand le lien est coupé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-020), HRT-05
maj: 2026-10-08
---

# BR-AUDIT-020 — Rechargement manuel quand le lien est coupé

## Règle
Côté interface : aucune relance automatique ; un bouton de rechargement manuel. Côté agent : `GET /audit` est sans état et idempotent ; recharger rend la page la plus récente.

## Application (code)
- Sans objet côté agent.
- `apps/desktop/src/components/organisms/OfflineBanner.vue` (« Réessayer maintenant », `link.retryNow`) puis `apps/desktop/src/stores/audit.ts::useAuditStore::resume` au retour du lien
- `apps/desktop/src/pages/Audit.vue`

## Interface
Aucune relecture automatique pendant la coupure (les lectures hors « Connecté » échouent sans rien envoyer). Le rechargement manuel est le bouton « Réessayer maintenant » du bandeau hors ligne (une tentative de reconnexion), puis relecture au retour du lien ; la page n'a plus de second bouton « Rechargement manuel » ni de message « Serveur toujours injoignable » (HRT-38, C30 : un seul geste hors ligne). Décision de Claude, à confirmer par Voiky : la règle écrite parlait d'un bouton propre à la page.

## Vérification
- `apps/desktop/src/pages/audit.test.ts` (hors ligne : un seul « Réessayer », celui du bandeau) et `apps/desktop/src/stores/audit.test.ts` (relecture au retour du lien).

## Cas limites
- Aucun cas limite propre à l'agent.

## Règles liées
- BR-AUDIT-011.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » et pointeurs du client (HRT-14, session 2026-10-04-hearth-creation).
- 2026-10-08 — le bouton de la page est retiré, le bandeau porte le geste (HRT-38).
