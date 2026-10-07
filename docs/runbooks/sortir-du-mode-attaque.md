# Sortir du mode attaque depuis le serveur

Quand un administrateur est enfermé dehors par le mode attaque (plus aucun poste reconnu, ou aucun poste avec une clé inscrite : activer et désactiver depuis le client exigent la clé, BR-TRUST-028). ADR-0025, BR-TRUST-027.

## Qui peut le faire
Quiconque **peut lire le dossier de données de l'agent** : en pratique `root` (ou l'utilisateur du service) sur la machine elle-même. La commande ouvre la base du dossier de données, que l'agent refuse quand il est ouvert aux autres ; elle n'a aucun contrôle de droits à elle, c'est le système de fichiers qui garde. Elle ne passe pas par le réseau, et une session volée ne la lance pas.

## Étapes
1. Sur le serveur : `sudo hearth-agent attack-mode status` (ajouter `--data-dir <dossier>` si le dossier n'est pas celui du service). Il dit `mode : active`, `suspended` (et `resumes_in_s`) ou `off`, et l'identifiant de l'activation.
2. `sudo hearth-agent attack-mode off`. Consigné au journal (`attack_mode.disable`, origine « ligne de commande du serveur »). Le service en marche le voit tout de suite (il relit la ligne à chaque décision) ; les clients voient l'état au contrôle suivant de leur flux (5 s).
3. Il n'existe pas de commande pour activer : activer exige un poste avec sa clé.

## Autres sorties
- Attendre : le mode s'arrête seul après 30 minutes sans tentative refusée (les tentatives de poste légitimes bloqués repoussent cette sortie).
- Redémarrer physiquement la machine : fenêtre de 30 minutes en régime d'alerte, puis le mode reprend.

## Diagnostic
- `status` rend `mode : off` alors que le client se croit enfermé : la session est présentée seule à la fin du mode ? Elle refonctionne sans reconnexion.
- La commande échoue sur le dossier de données : droits (lancer en `root`) ou mauvais `--data-dir`.
