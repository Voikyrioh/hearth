---
id: FIX-01M5P0Q7C3N8T5W2R9XK4D6HJE
titre: Le test de mise à jour de l'agent tombait au hasard sur le runner quand `systemctl kill` sortait en erreur (HRT-206)
date_découverte: 2026-10-24
date_correction: 2026-10-24
---

# FIX-01M5P0Q7C3N8T5W2R9XK4D6HJE : `systemctl kill` en erreur arrêtait le scénario de mise à jour

## Symptôme
Le pas « Mise à jour de l'agent à distance de bout en bout » du job `agent-install` échouait par intermittence (runs 39948183610, 40071073313, branche ci/HRT-168). Journal du run 40071073313 : `Sending signal SIGKILL to process 1873 (tokio-rt-worker) on client request`, `Failed to kill unit hearth-agent.service: Failed to send signal SIGKILL to auxiliary processes: Input/output error`, `Main process exited, code=killed`, puis `erreur : docker exec a échoué (exit status: 1)`.

## Cause root
Défaut du test, pas du produit. Le SIGKILL est celui du scénario (section 6, « agent tué pendant le téléchargement »), voulu. `systemctl kill` envoie le signal au processus principal puis parcourt le cgroup ; sur le systemd du conteneur jetable, ce parcours sort en EIO quand un fil disparaît, et la commande rend 1 alors que l'agent est bien tué. Sous `set -eu`, le script s'arrêtait sans `die` : aucune sortie, d'où le seul « docker exec a échoué ». L'agent avait déjà conclu proprement (« reprise par un superviseur », « Succeeded ») avant.

## Reproduction
Non reproductible à la demande : la course tient à un fil (`tokio-rt-worker`, pid 1873) qui disparaît pendant que `systemctl kill` parcourt le cgroup. Preuve dans le journal du run 40071073313 : `Sent signal SIGKILL to main process 1867`, `Sending signal SIGKILL to process 1873 (tokio-rt-worker)`, `Failed to send signal SIGKILL to auxiliary processes on client request: Input/output error`, puis sortie du script sans `die`. Pour la voir : exécuter `cargo xtask e2e-update` en boucle sur un runner chargé ; un passage réussi (run 40064379383) montre que le reste du scénario est sain.

## Impacté
`deploy/e2e/scenario-update.sh` (sections 6 et 7), `deploy/e2e/double-failure-systemd.sh` (deux morts du superviseur).

## Workaround
Relancer le job.

## Correction
`deploy/e2e/lib.sh` (sourcé par les deux scripts) porte `kill_main` : SIGKILL au pid principal, sans parcours du cgroup, réessai tant que l'unité n'a pas de pid, refus d'un pid vide ou 0 (`kill -KILL 0` tuerait le groupe, script compris ; MainPID vaut 0 entre deux relances). `report_exit`, appelé par le trap EXIT de `scenario-update.sh` et par `cleanup` de `double-failure-systemd.sh`, affiche le code, `$OUT` et la fin du journal pour toute sortie hors `die`, afin qu'un prochain échec dise pourquoi. Le produit n'est pas modifié : l'arrêt propre (SIGTERM, « agent arrêté ») lors de l'échange est déjà observé dans les journaux.

## Références
- Ticket : HRT-206
- Code : `deploy/e2e/scenario-update.sh`, `deploy/e2e/double-failure-systemd.sh` `deploy/e2e/lib.sh` (marqueur `FIX:01M5P0Q7C3N8T5W2R9XK4D6HJE`)
