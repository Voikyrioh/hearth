---
id: FIX-01M4VNGP6RMTHA7XNDDGV2FH1R
titre: L'état « à confirmer » d'une adoption ne survivait pas à un redémarrage, et le passage de relecture jugeait « non relue » une adoption faite pendant ses lectures
date_découverte: 2026-10-13
date_correction: 2026-10-14
---

# FIX-01M4VNGP6RMTHA7XNDDGV2FH1R : état « à confirmer » en mémoire seulement, jugement sur l'ensemble actuel (HRT-80, revue de la PR 89)

## Symptôme
1. Après un redémarrage de l'agent, toutes les adoptions étaient rechargées pleinement actives : une adoption « à confirmer » redevenait publiée, visible et gérable sans aucun acte, et le délai de 24 h repartait (un agent redémarré plus souvent qu'une fois par jour ne laissait jamais tomber une adoption non confirmée).
2. Une adoption faite pendant que `poll_units` lisait les unités (verrou relâché) était jugée « non relue » par ce passage, passait « à confirmer » et disparaissait juste après une réponse `200`.
3. `reload_adopted` relisait une clé en file d'effacement (adoption tombée, ligne pas encore effacée) et lui rendait tous ses droits.

## Reproduction
`services::tests::a_restart_keeps_an_adoption_to_confirm_and_its_deadline_never_restarts`, `a_restart_gives_no_right_to_an_adoption_whose_unit_was_replaced_while_the_agent_was_stopped`, `an_adoption_made_while_the_units_are_being_read_is_not_judged_by_that_pass` (faux superviseur suspendu dans sa lecture, sans délai réel), `a_reload_does_not_resurrect_an_adoption_waiting_to_be_erased`.

## Cause root
L'état vivait dans une `HashMap` de `Inner` sans colonne en base ; `poll_units` calculait les « non relues » sur `inner.adopted` au moment de la fin des lectures, pas à leur début ; `reload_adopted` ne tenait pas compte de `lapsed`.

## Impacté
Règle `BR-ADOPT-027` (marquée invariante). Pas un droit ouvert à un compte sans le droit : l'état est resté inaccessible à tout compte ; la conséquence était une adoption qu'on croyait fermée qui se rouvrait au redémarrage (il faut être root pour remplacer une unité), et une adoption neuve qui disparaissait.

## Workaround
Aucun.

## Correction
Migration `0014` (`adopted_services.unconfirmed_since`), écrite avant que l'adoption cesse d'ouvrir des droits, relue au démarrage, jamais réécrite ; une adoption par unité chargée au démarrage est « pas encore relue » (sans droit, sans délai) jusqu'à la première lecture de son unité ; le jugement se fait sur l'instantané du début du passage ; `reload_adopted` ignore la file d'effacement.

## Références
- Ticket : HRT-80 (revue de la PR 89)
- BR : BR-ADOPT-027, BR-ADOPT-017
