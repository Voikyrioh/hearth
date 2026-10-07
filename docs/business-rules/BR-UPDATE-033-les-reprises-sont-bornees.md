---
id: BR-UPDATE-033
domaine: UPDATE
titre: Les reprises du superviseur sont bornées, puis la reprise est abandonnée une seule fois
statut: active
invariant: true
source: HRT-27, ADR-0014 (amendement du 2026-10-07)
maj: 2026-10-07
---

# BR-UPDATE-033 : Les reprises du superviseur sont bornées, puis la reprise est abandonnée une seule fois

## Règle
Le marqueur d'étape (BR-UPDATE-032) compte les reprises du superviseur (`resumes`, écrit durablement à chaque reprise) **et les lancements de reprise que l'agent a ratés** (`launches`, écrit AVANT chaque tentative de `recover`, remis à zéro quand un superviseur démarre) : la borne porte sur leur somme (`Marker::attempts`). Un lancement qui échoue (`systemd-run` absent ou refusé) avance donc le compteur comme une reprise ; à la borne, l'agent conclut à son démarrage (`failed` / `rollback_failed`, copies gardées, traces de travail retirées, marqueur `abandoned`, une entrée au journal d'activité) et ne retente plus. « Reprises déjà tentées » est une donnée de `Leftovers` (`recovery_attempts`) lue par `classify_orphan` ; `Job::recover` n'a plus qu'un sens (partir du contrôle) et un travail `recover` sans marqueur (agent d'avant) compte pour toutes les reprises. À `MAX_RESUMES` (3) reprises, le lancement suivant **abandonne** : le marqueur devient `abandoned` (non « réglé », avec le résultat décidé et son instant) **d'abord**, puis l'état « reprise abandonnée, copies gardées » est journalisé (`journalctl -u hearth-agent-update`) **au plus une fois** (la reprise d'un abandon ne journalise plus : tué entre le marqueur et la ligne, il n'y a aucune ligne, jamais deux), puis l'abandon est réglé de façon idempotente : service relancé s'il était arrêté, résultat `failed` / `rollback_failed` écrit **une seule fois** (même instant ; l'agent le consigne au journal d'activité et l'annonce : c'est la trace qui ne manque jamais), marqueur « réglé », et **en dernier** `job.json` et `state.json` retirés (tant qu'ils sont là, une relance règle ce qui reste), **les copies (ancien binaire, base d'avant) sont gardées** pour la reprise à la main (runbook), et le service est relancé s'il a pu être arrêté par le superviseur (nouveau binaire sur sa base, ou ancien sur la sienne : couple cohérent). Plus rien ne relance : le superviseur sort en 0 ; un lancement de plus (unité relancée, agent redémarré) lit `abandoned` et sort sans rien dire ni rien toucher ; l'agent ne relance pas de reprise (plus de `job.json`). Le compteur de systemd (`StartLimitBurst=6` sur 600 s) est un second garde-fou : il se perd avec l'unité transitoire, c'est pourquoi le compteur qui compte est celui du disque.

Une nouvelle mise à jour repart de zéro : l'agent retire le marqueur (`discard_marker`) avant d'écrire le nouveau travail.

## Application (code)
- `crates/hearth-agent/src/domain/update/resume.rs::{MAX_RESUMES, enter}` ; `application/update_supervisor.rs::Supervisor::{abandon, abandon_unreadable, settle_abandoned}` ; `application/update.rs::UpdateService::execute` (`discard_marker`).

## Vérification
- `domain::update::resume::tests::the_resumes_are_bounded`, `an_abandoned_work_does_nothing_and_says_nothing_more`.
- `tests/update_use_cases.rs::a_recovery_whose_launch_keeps_failing_is_bounded_across_agent_restarts_then_concluded` (rouge sans le compteur de lancements), `domain::update::orphan::tests::a_recovery_is_tried_again_under_its_bound_and_never_at_it_whatever_the_job_says`, `tests/update_supervisor_resume.rs::an_abandon_killed_at_any_point_is_settled_by_the_next_start_with_one_result`, `two_stops_by_hand_in_a_row_do_not_exhaust_the_resumes`, `resumes_are_bounded_then_the_work_is_abandoned_once_and_nothing_relaunches_it`, `a_new_update_starts_from_zero_even_after_an_abandoned_one`.

## Cas limites
- Une écriture de marqueur qui échoue (disque plein) fait sortir le superviseur en code non nul : il est relancé, et ne peut pas compter ; c'est alors `StartLimitBurst` de systemd qui borne.
- Un travail **absent** à la relance (déjà conclu ou abandonné) : sortie en 0, rien à faire. Un travail **illisible** : jamais en silence : le résultat `failed` / `interrupted` est écrit (version connue si le travail se lit en partie ; `last.json`, annoncé et consigné par l'agent comme toute tentative interrompue), puis sortie en 0 (une relance n'y changerait rien) ; si ce résultat ne s'écrit pas, sortie non nulle (bornée par `StartLimitBurst`). Le client voit « La mise à jour de l'agent a été interrompue » (BR-UPDATE-028).

## Règles liées
- BR-UPDATE-028, BR-UPDATE-030, BR-UPDATE-032, ADR-0014

## Historique
- 2026-10-07 : création (HRT-27).
