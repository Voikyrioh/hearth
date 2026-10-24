---
id: FIX-01M5P0Q7C3N8T5W2R9XK4D6HJE
titre: Le test de mise à jour de l'agent tombait au hasard sur le runner quand `systemctl kill` sortait en erreur (HRT-206)
date_découverte: 2026-10-24
date_correction: 2026-10-24
---

# FIX-01M5P0Q7C3N8T5W2R9XK4D6HJE : `systemctl kill` en erreur arrêtait le scénario de mise à jour

## Symptôme
Le pas « Mise à jour de l'agent à distance de bout en bout » du job `agent-install` échouait par intermittence (runs 39948183610, 40071073313, branche ci/HRT-168). Journal : `Failed to kill unit hearth-agent.service: Failed to send signal SIGKILL to auxiliary processes: Input/output error`, `Main process exited, code=killed`, puis `erreur : docker exec a échoué (exit status: 1)`.

## Cause root
Défaut du test, pas du produit. Le SIGKILL est celui du scénario (section 6, « agent tué pendant le téléchargement »), voulu. `systemctl kill` envoie le signal au processus principal puis parcourt le cgroup ; sur le systemd du conteneur jetable, ce parcours sort en EIO quand un fil disparaît, et la commande rend 1 alors que l'agent est bien tué. Sous `set -eu`, le script s'arrêtait sans `die` : aucune sortie, d'où le seul « docker exec a échoué ». L'agent avait déjà conclu proprement (« reprise par un superviseur », « Succeeded ») avant.

## Impacté
`deploy/e2e/scenario-update.sh` (sections 6 et 7), `deploy/e2e/double-failure-systemd.sh` (deux morts du superviseur). Fréquence croissante avec la charge du runner.

## Workaround
Relancer le job.

## Correction
Le SIGKILL vise le pid du processus principal (`kill_main`, `kill -KILL`), sans parcours du cgroup, avec réessai tant que l'unité n'a pas de pid. Un `trap EXIT` affiche le code et la fin du journal pour toute sortie hors `die`, afin qu'un prochain échec dise pourquoi. Le produit n'est pas modifié : l'arrêt propre (SIGTERM, « agent arrêté ») lors de l'échange est déjà observé dans les journaux.

## Références
- Ticket : HRT-206
- Code : `deploy/e2e/scenario-update.sh`, `deploy/e2e/double-failure-systemd.sh` (marqueur `FIX:01M5P0Q7C3N8T5W2R9XK4D6HJE`)
