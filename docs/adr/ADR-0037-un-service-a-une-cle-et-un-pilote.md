---
id: ADR-0037
titre: Un service a une clé stable et un pilote (moteur ou superviseur systemd détecté) ; le moteur est la vérité, la base de l'agent ne retient que ce que lui seul sait
type: architecture
statut: proposée (non validée par Voiky ; posée avec HRT-61, détection implémentée avec HRT-63)
date: 2026-10-13
portee: repo
remplace: aucune
liens: [ADR-0006, ADR-0012, ADR-0036, conception technique 2026-10-13 (déploiements, lot 1) sections 0, 6 et 9, BR-SVC-003, BR-SVC-005, règles de la spec « cycle de vie » et d'identité stable de la spec « voir les services »]
---

# ADR-0037 : Clé et pilote d'un service

## Contexte
Sur forge, chaque conteneur est déclaré par `virtualisation.oci-containers` : une unité `docker-<nom>.service` lance `docker run --rm --name=<nom>` au premier plan, supprime le conteneur avant chaque démarrage et après chaque arrêt, et se relance (`Restart=always`). Piloté par l'API du moteur, un tel conteneur est supprimé à l'arrêt puis recréé par systemd dix secondes plus tard sous un autre identifiant ; arrêté, il n'existe plus pour le moteur ; son délai d'arrêt déclaré (120 s pour un serveur de jeu) serait écrasé. Le même schéma existe sur toute distribution où une unité systemd lance un conteneur.

## Décision
1. **Clé de service** `ServiceKey`, opaque pour le client : `h_<ULID>` (étiquette `io.hearth.service-id` d'un service créé par Hearth), `u_<unité>` (service supervisé par une unité systemd), `c_<identifiant>` (tout autre conteneur). Toute route, tout acte signé, tout verrou et toute ligne de base porte la clé, jamais l'identifiant du conteneur seul.
2. **Superviseur détecté de façon générique**, port `ServiceSupervisor` : un processus client du moteur (`docker`, `podman`) dont la ligne de commande nomme le conteneur et dont le groupe de contrôle est celui d'une unité `.service`. Aucune connaissance de NixOS, aucun motif de nom d'unité.
3. **Pilote** `ServiceDriver` : `Supervisor { unit }` pour un service supervisé (`systemctl start|stop|restart`), `Engine` sinon. Un service supervisé n'est jamais piloté par l'API du moteur.
4. **Délai d'arrêt** : celui du service (`Config.StopTimeout`, ou `TimeoutStopUSec` de l'unité), 10 s à défaut.
5. **Vérité** : le moteur pour l'existence, les réglages et l'état ; systemd pour l'état d'un service supervisé dont le conteneur est absent. La base de l'agent ne garde que : l'arrêt voulu depuis Hearth (`service_intents`), les actions et déploiements des dernières 24 h (`service_actions`, `deployments`), la dernière vue des services supervisés (`supervised_services`). Aucune copie de la configuration d'un conteneur, aucune valeur de variable, aucune commande.
6. **Hearth n'écrit rien dans systemd** : ni `disable`, ni `mask`, ni unité.

## Comment l'appliquer
- `domain/services/key.rs`, `state.rs` (`derive_state`, fonction pure), `lock.rs`, `intent.rs` ; `application/ports/service_supervisor.rs` ; `infrastructure/supervisor/{proc_systemd,none}.rs`.
- Le nom d'unité lu dans `/proc` est validé puis passé en argument à `systemctl`, jamais par un interpréteur.
- Compter les relances par les démarrages observés et non demandés par Hearth, pas par `RestartCount` du moteur.
- Interface : repère « Géré par le système ({unité}) » ; confirmation d'arrêt avec le délai réel et le retour au prochain redémarrage ou à la prochaine application de la configuration.

## Quand NE PAS l'appliquer / limites
- Un service supervisé jamais vu en marche par Hearth est invisible jusqu'à son premier démarrage.
- Après un arrêt, un service supervisé dont le conteneur est supprimé n'a plus de journaux lisibles par le moteur.
- Sans systemd (adaptateur `none`), tout service est piloté par le moteur ; un superviseur d'une autre nature n'est pas détecté et le service peut « repartir tout seul », ce que Hearth annonce.
- Pile Compose : simple information (`com.docker.compose.project`), pilote `Engine`.

## Mise en oeuvre (HRT-61, HRT-63)
- **HRT-61 (lecture seule)** : `domain/services/key.rs` (`ServiceKey`) et `state.rs` (`derive_state`). Seule la clé `c_<identifiant complet>` est PRODUITE ; `h_<ULID>` et `u_<unité>` sont seulement analysées. Aucun superviseur, aucun port `ServiceSupervisor`, aucune table : tout service a le pilote du moteur (le champ `driver` du fil reste absent). Les démarrages non demandés sont comptés par l'agent (fenêtre glissante de 10 minutes en mémoire, un démarrage = un `StartedAt` distinct vu dans une inspection), jamais par `RestartCount` du moteur.
- **HRT-63 (détection, sans action)** : port `ServiceSupervisor` (`owners`, `unit` : ni démarrer, ni arrêter, ni redémarrer, qui arrivent avec HRT-64), adaptateur `infrastructure/supervisor/proc_systemd.rs`, clé `u_<unité>` (le suffixe `.service` fait partie de la clé : `u_docker-terraria.service`), champs `driver`, `unit`, `grace_s` du fil, table `supervised_services` (migration `0009`, qui crée aussi `service_intents`, `service_actions`, `deployments`, `audit_events.details`, non utilisées ici), service supervisé gardé dans la liste d'après l'état de son unité.
  - **Détection** : pour chaque NOUVEAU conteneur (une fois pour sa vie, mise en cache), l'agent parcourt `/proc`, cherche un processus dont le programme est `docker` ou `podman` (cible du lien `exe`, à défaut `argv[0]` ; une enveloppe `.docker-wrapped` compte), dont la ligne de commande est un `run` (son `--name`, le premier fait foi) ou un `start` (ses arguments) qui désigne le conteneur par nom, identifiant complet ou préfixe d'au moins 12 caractères, et lit le `cgroup` de ce processus : le premier composant `.service` du chemin, sauf `user@*.service` (gestionnaire d'utilisateur) et tout chemin qui passe par un `.scope` (session). Ce n'est qu'une REVENDICATION : le client doit être le processus principal de l'unité (`MainPID` de `systemctl show`, lu pour chaque unité revendiquée), sinon un service qui lance des conteneurs au premier plan (tâche planifiée, exécuteur de CI) passerait pour eux ; sans réponse de `systemctl`, aucune unité n'est crue. Deux unités qui revendiquent le même conteneur : aucun propriétaire. Une unité qui revendique plusieurs conteneurs (`docker start -a a b`) ou à laquelle deux conteneurs VIVANTS sont attribués ne supervise aucun d'eux : ils restent listés chacun sous leur clé `c_` (rien n'est caché derrière une clé commune, BR-SVC-001). Un `docker run -d` dont le client a quitté n'est pas détecté (limite). Limite restante : le critère `MainPID` ne reconnaît pas un client lancé par un enrobage qui ne fait pas `exec` (`sh -c 'docker run ...'` sans `exec` : le principal est `sh`) ; ce conteneur reste `c_`.
  - **Sécurité** : le nom d'unité est lu sur le système, donc VALIDÉ (`UnitName`) avant d'aller dans une clé, un argument ou la base. `systemctl show --property=... -- <unité>` est lancé sans interpréteur, un argument à la fois, avec un délai de 5 s ; son absence est un cas normal et silencieux. Lecture seule de `/proc` : 100 000 processus et 2 s au plus par recherche.
  - **Mémoire** : `supervised_services.last_view_json` ne contient que les réglages (nom, image, ports, montages, suppression à l'arrêt, délai) : ni nom de variable, ni commande, ni valeur. L'état de chaque unité connue est relu toutes les deux périodes de sonde (4 s). Une unité que le système ne connaît plus, dont le conteneur n'existe plus, est oubliée avec sa ligne (pas de purge horaire séparée).
  - **Écarts avec la conception** : (1) la clé garde le suffixe `.service` ; (2) le code de sortie d'un service supervisé sans conteneur est celui du dernier arrêt VU par l'agent, jamais relu de `ExecMainStatus` ; (3) la commande affichée est `Path` + `Args` (ce que le conteneur lance), le moteur ne disant pas si elle vient de l'image.
  - **Résultat réel de la section 0 (course CI de la tête `cbf7618`, job `engine`, étape `supervised`, vrai moteur Docker et vrai systemd)** : `systemd-run --unit=hearth-e2e --property=Restart=always --property=RestartSec=3 docker run --rm --name hearth-e2e busybox sleep 600`. Déjà PROUVÉ par cette première course : (1) le propriétaire du conteneur est trouvé par le vrai `/proc` (unité `hearth-e2e.service`), `systemctl show` réel est lu, le registre complet (vrai `bollard`) liste le service sous `u_hearth-e2e.service`, en marche ; (2) un arrêt par l'API du moteur (`docker stop`, appel accepté, code 0) NE TIENT PAS : le conteneur est supprimé par `--rm` 2383 ms après l'ordre, l'unité passe en `activating/auto-restart` (`NRestarts` de 0 à 1) puis relance un conteneur d'un AUTRE identifiant 5906 ms après l'ordre (`RestartSec=3`) ; l'état « Arrêté » n'est jamais observé ; (3) suite d'états du registre pour cet arrêt unique : `Running, Stopping, Starting, RestartLoop, Running` (le « RestartLoop » pour une seule relance était un défaut, corrigé : une relance n'est pas une boucle, BR-SVC-006) ; (4) la clé `u_hearth-e2e.service` est restée listée pendant toute la séquence. L'hypothèse de la section 0 est confirmée : piloter une unité `Restart=` par l'API du moteur est inopérant, d'où la décision (piloter par le superviseur). Ces chiffres viennent de la course CI de `cbf7618` seule ; ils sont relus dans le journal de l'étape. **Reste à consigner à la prochaine course** : l'arrêt PAR LE SUPERVISEUR (`systemctl stop`) : la première course a échoué à cette étape, après environ 90 s, parce que le conteneur de test (`sleep` est le processus 1 et ignore SIGTERM) a laissé l'unité attendre le `TimeoutStopSec` par défaut ; le test a été corrigé (conteneur qui s'arrête au signal, `TimeoutStopSec=20` déclaré, comme un serveur de jeu) et consigne maintenant, pour une unité transitoire ET pour une unité écrite dans un fichier (le cas de NixOS), la durée de `systemctl stop`, l'état de l'unité, la présence du conteneur et la suite d'états du registre. Ce que fait `systemctl stop` sur une unité dont l'`ExecStart` est un `docker run` nu, SANS `ExecStop`, avec un conteneur qui ignore SIGTERM (90 s, puis SIGKILL du client ; conteneur éventuellement orphelin) n'est pas consigné : « piloter par le superviseur » est démontré pour les unités du module NixOS (`ExecStop=docker stop`) et reste à instruire pour une unité écrite à la main avant HRT-64.

## Alternatives rejetées
- **Piloter par l'API du moteur et annoncer « reparti tout seul »** : aucun arrêt ne tiendrait sur forge, aucun démarrage possible, sauvegarde d'un serveur de jeu coupée.
- **Reconnaître les unités par leur nom (`docker-*.service`)** : propre à NixOS.
- **Identité par nom de conteneur** : un conteneur supprimé puis recréé par un tiers sous le même nom serait confondu (règle d'identité stable de la spec « voir les services », HRT-63).
- **État voulu tenu dans la base de l'agent et réconcilié au démarrage** : deux vérités, secrets en base ; à reconsidérer quand un service devra être recréé (mise à jour, duplication).

## Conséquences
- Plusieurs règles des specs « voir les services » et « cycle de vie » sont à amender : la liste, règle par règle, est dans la section « Écarts avec les specs fonctionnelles » de la conception technique.
- Migration `0009_services.sql`.
- Un test CI avec un vrai systemd et un vrai moteur (`systemd-run` d'un `docker run --rm`) : `tests/engine_supervised.rs`, étape `supervised` du job `engine`.

## Références
- nixpkgs, `nixos/modules/virtualisation/oci-containers.nix`.
- `forge-nixos` : `docker.nix`, `jeux/terraria.nix`, `jeux/7dtd.nix`.
- `systemd.service(5)`, `systemctl(1)`.
