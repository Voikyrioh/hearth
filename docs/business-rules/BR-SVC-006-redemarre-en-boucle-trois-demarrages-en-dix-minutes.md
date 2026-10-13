---
id: BR-SVC-006
domaine: SVC
titre: « Redémarre en boucle » : 3 démarrages non demandés en 10 minutes
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-006), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-006 — « Redémarre en boucle » : 3 démarrages non demandés en 10 minutes

## Règle
Un service est « redémarre en boucle » après au moins 3 démarrages non demandés dans les 10 dernières minutes, que ce soit par le moteur, par le système du serveur (unité systemd) ou par autre chose, ou quand le moteur dit qu'il est en cours de relance (`restarting`), code de sortie zéro compris. La spec le dit ainsi (BR-SVC-006 : « ou quand il vient de s'arrêter et qu'il est en attente de relance, quel que soit son code de sortie ») : un service dont le MOTEUR dit « en cours de relance » (`restarting`) est donc « en boucle » dès la première relance, ce n'est pas un élargissement de la fiche pour suivre le code. **Service supervisé par une unité** : une unité qui attend de se relancer (`SubState=auto-restart`) après un arrêt est « Démarrage », PAS « Redémarre en boucle » : la boucle vient du compteur (3 démarrages non demandés en 10 minutes, comptés par clé `u_`), jamais d'une seule attente de relance. La conception technique le dit ainsi (« unité activating -> starting ; restart_loop si elle relance après un échec » : un échec répété, que le compteur mesure). La première course CI réelle a montré le défaut : UN arrêt donnait `Running, Stopping, Starting, RestartLoop, Running`. Les démarrages sont comptés par l'agent (fenêtre glissante en mémoire, un démarrage = un `StartedAt` distinct), jamais par `RestartCount` du moteur. `restarts_10m` donne le compte.

## Application (code)
- `crates/hearth-agent/src/domain/services/state.rs::derive_state`
- `crates/hearth-agent/src/domain/services/state.rs::StartWindow`
- `crates/hearth-agent/src/domain/services/state.rs::RESTART_LOOP_STARTS`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::commit`

## Vérification
- `crates/hearth-agent/src/domain/services/state.rs::tests::a_running_service_loops_from_three_unrequested_starts`
- `crates/hearth-agent/src/domain/services/state.rs::tests::one_wait_for_a_relaunch_is_a_start_and_only_the_count_makes_a_loop`
- `crates/hearth-agent/src/application/services.rs::tests::three_relaunches_by_the_system_are_a_loop_one_is_not`
- `crates/hearth-agent/src/domain/services/state.rs::tests::the_start_window_counts_ten_minutes_and_each_start_once`
- `crates/hearth-agent/src/application/services.rs::tests::three_starts_in_ten_minutes_make_a_restart_loop_and_old_ones_do_not_count`

## Cas limites
- Au démarrage de l'agent, le `StartedAt` d'un conteneur qui tourne déjà est noté : un service en boucle depuis avant le redémarrage de l'agent est vu en boucle dès qu'il a 3 démarrages dans la fenêtre.

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
- 2026-10-13 — HRT-63 : la fenêtre est tenue par CLÉ de service (`u_` pour un service supervisé) : les démarrages d'un conteneur que le système recrée se cumulent ; une unité en attente de relance (`auto-restart`) dont le conteneur n'existe pas est « Démarrage » ; elle devient « redémarre en boucle » quand le compteur atteint trois (revue de la PR 81, B5).
