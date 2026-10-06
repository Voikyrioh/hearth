---
id: BR-AUDIT-013
domaine: AUDIT
titre: Les refus en rafale sont regroupés à l'affichage
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-013), HRT-05
maj: 2026-10-06
---

# BR-AUDIT-013 — Les refus en rafale sont regroupés à l'affichage

## Règle
Côté interface : au moins 5 connexions refusées en 2 minutes depuis la même origine se regroupent sur la page chargée (« X tentatives refusées en Y min »). L'agent fournit ce qu'il faut : adresse de la connexion, action, résultat, date. Seuil fixé par la conception technique (§7).

## Application (code)
- `hearth-proto::api::audit::AuditEventItem` (champs `origin.addr`, `action`, `outcome`, `at`).
- `apps/desktop/src/audit/grouping.ts::{groupBursts, BURST_MIN_ATTEMPTS, BURST_WINDOW_MS}` (UNE source côté interface)
- `apps/desktop/src/audit/rows.ts::displayRows`

## Interface
Au moins 5 tentatives de connexion REFUSÉE depuis la même adresse, étalées sur 2 minutes au plus et qui se suivent dans la liste, forment une ligne « X tentatives refusées en Y min » (Y arrondi au-dessus, au moins 1), déployable au clic, à Entrée, ou aux flèches droite et gauche. Une entrée de synthèse de l'agent (`repeat_count`) compte pour `1 + repeat_count`, et une seule entrée n'est jamais un regroupement. Une entrée d'un autre type, d'une autre adresse ou d'un autre résultat (et le blocage temporaire) interrompt la rafale : rien n'est masqué ni déplacé. Les fenêtres sont ancrées sur la tentative la plus ancienne : une nouvelle tentative en tête ne change pas les rafales plus anciennes.

## Vérification
- `apps/desktop/src/audit/audit.test.ts` (« regroupement des rafales »).
- `apps/desktop/e2e/audit.spec.ts` (« une rafale de refus est regroupée… »).

## Cas limites
- Le regroupement ne porte pas sur le compte : l'identifiant saisi n'est pas retenu (BR-AUDIT-006).

## Règles liées
- BR-AUDIT-006.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » et pointeurs du client (HRT-14, session 2026-10-04-hearth-creation).
