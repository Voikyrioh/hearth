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
`LinkManager::logout` relâche le verrou du serveur pendant l'appel réseau puis effaçait sans condition le jeton du coffre à la reprise du verrou, sans voir qu'un `login` avait rangé un jeton neuf entre-temps ; la tâche du serveur faisait de même en traitant la commande `LoggedOut`.

## Impacté
Depuis HRT-10 (BR-CONN-016), quand `login` s'intercale dans la fenêtre de l'appel réseau de `logout`.

## Workaround
Aucun.

## Correction
`login` remet « déconnecté » à faux sous le même verrou en rangeant son jeton : à la reprise du verrou, `logout` ne touche à rien si le carnet n'est plus « déconnecté » (aucune comparaison de jetons). La tâche du serveur, qui traite la commande `LoggedOut` plus tard, efface le jeton si le carnet dit encore « déconnecté » : LECTURE ET EFFACEMENT sous le verrou d'écriture du serveur (`task.rs::erase_token_if_signed_out`, `try_lock`, réessai dans un instant si une connexion ou une déconnexion le tient, jamais d'attente du verrou pour ne pas bloquer la boucle), le même que celui de `login`, qui range son jeton PUIS lève « déconnecté » sous ce verrou : une connexion ne peut plus s'intercaler, la course est fermée (round 2 de la review). Tests `tracking.rs::a_slow_logout_never_erases_the_token_of_a_login_that_came_in_between`, `::a_task_late_on_a_logout_never_erases_the_token_of_a_login_that_already_came_in` et `::a_late_task_never_erases_a_token_stored_by_a_login_that_has_not_yet_cleared_signed_out` (la fenêtre exacte : coffre qui retient `login` après le rangement du jeton ; rouge sans le verrou, vérifié par mutation) ; le dernier garde une fenêtre d'observation de 200 ms pour l'absence d'effacement.

## Références
- Ticket : HRT-12 (suivi de la review de HRT-10, PR #12)
- BR : BR-CONN-016
