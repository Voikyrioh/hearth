---
id: FIX-01M46G7Z0ZP43T53M2F5KG4VKS
titre: La fin de `logout` effaçait le jeton d'une reconnexion intervenue entre-temps
date_découverte: 2026-10-05
date_correction: 2026-10-05
---

# FIX-01M46G7Z0ZP43T53M2F5KG4VKS : La fin de `logout` effaçait le jeton d'une reconnexion intervenue entre-temps

## Symptôme
Déconnexion lente (serveur lent à répondre) puis nouvelle connexion avant la fin de l'appel : l'utilisateur se retrouve connecté à l'écran mais sans jeton au coffre, donc sans session au prochain démarrage.

## Cause root
`LinkManager::logout` relâche le verrou du serveur pendant l'appel réseau puis effaçait sans condition le jeton du coffre à la reprise du verrou, sans voir qu'un `login` avait rangé un jeton neuf entre-temps.

## Impacté
Depuis HRT-10 (BR-CONN-016), quand `login` s'intercale dans la fenêtre de l'appel réseau de `logout`.

## Workaround
Aucun.

## Correction
À la reprise du verrou, si le carnet n'est plus « déconnecté » ou si le jeton du coffre n'est plus celui qu'on vient de fermer, `logout` ne touche à rien. Test `tracking.rs::a_slow_logout_never_erases_the_token_of_a_login_that_came_in_between` (rouge avant le correctif).

## Références
- Ticket : HRT-12 (suivi de la review de HRT-10, PR #12)
- BR : BR-CONN-016
