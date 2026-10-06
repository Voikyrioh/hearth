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
Côté interface : au moins 5 connexions refusées en 2 minutes depuis la même origine se regroupent sur la page chargée (« X tentatives refusées en Y min »). L'agent fournit ce qu'il faut : adresse de la connexion, action, résultat, date. Seuils : au moins 5 tentatives en 2 minutes au plus (`BURST_MIN_ATTEMPTS`, `BURST_WINDOW_MS`), CHOIX DU DÉVELOPPEUR, la spec les laisse « à définir » : à confirmer par le détenteur du produit.

## Application (code)
- `hearth-proto::api::audit::AuditEventItem` (champs `origin.addr`, `action`, `outcome`, `at`).
- `apps/desktop/src/audit/grouping.ts::{groupBursts, BURST_MIN_ATTEMPTS, BURST_WINDOW_MS}` (UNE source côté interface)
- `apps/desktop/src/audit/rows.ts::displayRows`

## Interface
Ce que fait l'agent (BR-AUDIT-007, `repeat.rs`) : les refus de même clé (compte, action, résultat, cible, raison ; **jamais l'adresse ni le poste**) d'une même minute sont condensés ; le premier est écrit seul (`repeat_count` 0), puis UNE synthèse qui EST la dernière occurrence et dont `repeat_count` la compte déjà. Une fenêtre de N + 1 refus = la première entrée + une synthèse à `repeat_count == N` : N tentatives, pas N + 1. Les N occurrences d'une synthèse peuvent venir d'adresses différentes (seule celle de la dernière est connue). L'entrée de débordement (« activité trop variée ») mêle des comptes et des cibles.

Un groupe « X tentatives refusées en Y min » ne ment jamais :
- **X exact** : somme de `max(1, repeat_count)` par entrée (aucune occurrence comptée deux fois ni oubliée) ; Y arrondi au-dessus, au moins 1 ;
- **l'adresse n'est affirmée (« depuis … ») que si elle est certaine** : toutes les entrées du groupe sont des occurrences ordinaires. Dès qu'une synthèse y entre, le décompte reste exact mais l'adresse n'est plus nommée (la clé de l'agent ne la contient pas, c'est voulu pour borner le nombre de groupes ; ce n'est pas un bug) ;
- membres : refus de connexion d'origine « client » dont l'adresse (celle de la dernière occurrence pour une synthèse) est la même, qui se suivent dans la liste, dans une fenêtre de 2 minutes ancrée sur la tentative la plus ancienne ; au moins 2 entrées et X ≥ 5. Une entrée d'un autre type, d'une autre adresse ou d'un autre résultat, le blocage temporaire et l'entrée de débordement interrompent la rafale et ne sont jamais absorbés ni déplacés ;
- **une rafale qui touche le bas de la liste alors qu'il en reste à charger n'est pas groupée** (son début est inconnu, le décompte serait partiel) : elle se groupe quand la page suivante arrive ;
- déployer : clic, Entrée, flèches droite et gauche ; l'état déployé se retient par identifiants d'entrées et survit à l'arrivée d'une entrée en tête ou d'une page sous la rafale.

Point à trancher par le détenteur du produit : la spec dit « depuis la même adresse IP » ; l'agent condense sans l'adresse (BR-AUDIT-007). L'interface reste vraie dans les deux cas. Mettre l'adresse dans la clé de l'agent changerait cette règle (non fait ici). **POINT OUVERT** : « même IP stricte dans la condensation de l'agent » à trancher par le détenteur du produit (question posée par l'agent principal).

## Vérification
- `apps/desktop/src/audit/audit.test.ts` (« regroupement des rafales » : compte exact avec synthèses, adresse non affirmée, adresses mêlées, débordement, groupe à cheval sur deux pages, limite d'affichage, entrée d'un autre type, état déployé stable).
- `apps/desktop/e2e/audit.spec.ts` (« une rafale de refus est regroupée… »).

## Cas limites
- Le regroupement ne porte pas sur le compte : l'identifiant saisi n'est pas retenu (BR-AUDIT-006).

## Règles liées
- BR-AUDIT-006.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » et pointeurs du client (HRT-14, session 2026-10-04-hearth-creation).
