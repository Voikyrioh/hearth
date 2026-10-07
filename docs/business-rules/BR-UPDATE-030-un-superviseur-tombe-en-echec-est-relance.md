---
id: BR-UPDATE-030
domaine: UPDATE
titre: Un superviseur de mise à jour tombé en échec est relancé et reprend là où il s'est arrêté
statut: active
invariant: true
source: HRT-27, ADR-0014 (amendement du 2026-10-07)
maj: 2026-10-07
---

# BR-UPDATE-030 : Un superviseur de mise à jour tombé en échec est relancé et reprend là où il s'est arrêté

## Règle
Le superviseur (BR-UPDATE-015) tourne dans une unité transitoire `systemd-run` qui porte `Restart=on-failure`, `RestartSec=2`, `StartLimitIntervalSec=600` et `StartLimitBurst=6`. S'il est tué (signal, gestionnaire de mémoire) ou sort avec un code non nul, systemd le relance **sans geste manuel** ; relancé, il lit son marqueur d'étape (BR-UPDATE-032) et reprend là où il s'est arrêté : le nouveau binaire qui ne démarre pas, après un superviseur tué, est donc remplacé par l'ancien et le service est relevé. Rien ne relance le superviseur dans les autres cas : une sortie à 0 (résultat écrit, reprise abandonnée, service arrêté à la main, un autre superviseur au travail, travail absent ou illisible) ou un `systemctl stop hearth-agent-update` ne le relancent pas. **Aucune minuterie systemd périodique** (une minuterie bouclerait après un retour arrière raté et déferait un arrêt voulu), **pas d'`OnFailure=`** (ADR-0014 : il lancerait le binaire dont la copie ou la lecture est en cause, sans lien avec l'unité transitoire qui n'existe plus). **Un seul arbitre de la reprise.** Entre la mort du superviseur et sa relance (2 s), le verrou est libre : un agent qui démarre à cet instant (le nouveau, qui plante et se relance toutes les 5 s) ne doit pas reprendre à la place de systemd. L'agent considère donc qu'un superviseur « travaille » tant que le verrou est tenu **ou** que son unité transitoire existe encore (`systemctl show hearth-agent-update`, `activating` = en attente de relance) avec un marqueur vivant sous sa borne (`UpdateHost::supervisor_pending`) : il surveille, sans toucher au travail (`job.json`) ni à la copie du superviseur. « Reprise déjà tentée » se lit dans le marqueur (reprises comptées), pas dans `job.recover` ; une reprise lancée par l'agent garde la copie du superviseur déjà déposée (l'ancien binaire), elle ne la réécrit pas avec le binaire courant. Après un redémarrage de la machine, l'unité n'existe plus : l'agent reprend la main (BR-UPDATE-034). Les bornes de systemd ne sont qu'un second garde-fou : la borne qui compte est celle du superviseur (BR-UPDATE-033).

**Installation gérée (NixOS)** : l'agent n'écrit aucune unité et ne suppose pas pouvoir en modifier une ; la mise à jour à distance n'y existe pas (`Updating::allowed` est faux dès `--managed` ou sans systemd, BR-UPDATE-011), donc il n'y a ni superviseur ni mécanisme de reprise : la mise à jour passe par la configuration du système. Le mécanisme n'utilise que `systemd-run`, une unité transitoire, et ne touche jamais à `hearth-agent.service`.

## Application (code)
- `crates/hearth-agent/src/infrastructure/update/host.rs::{systemd_run_arguments, FsUpdateHost::launch}` (les propriétés de l'unité transitoire).
- `crates/hearth-agent/src/app/update.rs::run_supervisor` (code de sortie : 0 ou non nul, ce qui commande la relance).
- `crates/hearth-agent/src/domain/update/resume.rs::{enter, Marker::can_resume}` (ce que fait un superviseur relancé) ; `application/update.rs::UpdateService::{supervised, read_leftovers, recover}` et `infrastructure/update/host.rs::FsUpdateHost::{supervisor_pending, existing_supervisor}` (l'arbitre).

## Vérification
- `infrastructure::update::host::tests::the_supervisor_unit_restarts_on_failure_only_and_never_on_a_timer`.
- `tests/update_use_cases.rs::an_agent_starting_while_systemd_is_about_to_restart_the_supervisor_leaves_the_work_alone`, `after_a_reboot_a_recovery_that_was_tried_but_is_under_its_bound_is_launched_again`, `a_recovery_whose_marker_reached_its_bound_is_concluded_for_good` ; `deploy/e2e/double-failure-systemd.sh` (section 4, vrai agent redémarré dans la fenêtre de relance).
- `tests/update_supervise_cli.rs` (codes de sortie du binaire réel).
- `tests/update_supervisor_resume.rs::a_supervisor_killed_at_every_point_of_a_successful_update_concludes_the_same_way`, `…_of_a_rollback_rolls_back_exactly_once`.
- Sur un vrai systemd : `deploy/e2e/scenario-update.sh` section 7 (jouée par la CI, `cargo xtask e2e-update`, Debian + systemd 252 ; aussi rejouée sous WSL2 le 2026-10-07) et `deploy/e2e/double-failure-systemd.sh` (lancé à la main sous systemd réel, dernière passe le 2026-10-07 sous WSL2 systemd 255 ; pas en CI).

## Cas limites
- Unité chargée mais marqueur à la borne : l'agent ne se croit pas supervisé et conclut de son côté (échec, copies gardées) pendant que le superviseur relancé va abandonner : deux autorités, même conclusion.
- `deactivating`, ou un `systemctl show` qui échoue : « pas supervisé » ; l'agent peut lancer sa reprise pendant que systemd relance. Sans dégât : la copie du superviseur n'est plus réécrite, le lancement échoue sur le nom d'unité déjà pris, la reprise de systemd lit le marqueur.
- Une unité transitoire disparaît au redémarrage de la machine : voir BR-UPDATE-034.
- `Restart=on-failure` ne relance pas un `systemctl stop` de l'unité du superviseur ni un `kill -TERM` : ce sont des arrêts propres pour systemd. Un SIGKILL, un SIGSEGV, un code de sortie non nul le relancent.

## Règles liées
- BR-UPDATE-015, BR-UPDATE-028, BR-UPDATE-031 à 034, ADR-0014, ADR-0012

## Historique
- 2026-10-07 : création (HRT-27).
