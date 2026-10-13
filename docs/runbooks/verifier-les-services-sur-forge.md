# Vérifier les services sur la forge

Squelette (HRT-61) du runbook de vérification des services sur le serveur forge. Il grossit à chaque tranche des déploiements (HRT-63, 64, 66, 68, 70) ; Voiky exécute et consigne (HRT-71). Cette première partie ne couvre que la tranche 1 : le moteur détecté et la liste en lecture seule. Rien dans cette tranche n'écrit dans le moteur ni dans la base : c'est sans risque pour les services qui tournent.

## Règles de sécurité de cette vérification

- Aucun geste sur Terraria sans autosave et sauvegarde préalables (règle du hub). Cette partie ne touche à aucun conteneur : elle ne fait que lire.
- Aucun push sur `main` du dépôt forge pendant la vérification (un push `main` déclenche le changement de configuration du serveur).
- La détection est faite avec les droits de l'agent : sur forge, l'agent est installé « géré » ; son compte et ses droits sur le socket du moteur sont à relever (étape 2).

## Partie 1 : le moteur détecté et la liste en lecture seule (tranche 1)

À exécuter après la mise à jour de l'agent forge avec la version qui porte HRT-61.

1. **Capacité.** Depuis le client Hearth ou `curl -sk https://<forge>:7341/api/v1/hello` : le corps contient `"capabilities":["services"]`. Sinon, l'agent est trop ancien.
2. **État du moteur.** Se connecter, puis `GET /api/v1/services` (ou l'écran des services quand HRT-62 est livré). Attendu : `engine.state` = `ready`, `name` = `Docker Engine…`, `agent_account` = le compte réel de l'agent. À consigner : le compte, `host.declarative` (doit être `true`, NixOS), `host.init` (`systemd`). Si `access_denied` : relever le compte de l'agent et le groupe du socket `/var/run/docker.sock` (sur NixOS, `virtualisation.docker.enable`) ; ne rien changer sans décision de Voiky.
3. **La liste.** Attendu : tous les conteneurs de forge, arrêtés compris (Terraria, 7 Days to Die, forge-studio, les conteneurs à carte graphique), triés par nom sans tenir compte de la casse, chacun avec son image, son état, ses ports. À consigner : les services manquants ou en trop par rapport à `docker ps -a` sur le serveur. Rappel (limite assumée, tranche 2) : un service géré par une unité systemd (`oci-containers`) qui est arrêté n'existe pas pour le moteur, donc n'apparaît pas tant qu'il n'a pas tourné.
4. **Aucune valeur de variable.** Vérifier que la réponse de `GET /api/v1/services` ne contient ni nom ni valeur de variable d'environnement (chercher, par exemple, une variable connue de forge-studio).
5. **En direct.** Avec l'abonnement au flux (ou en relançant `GET /api/v1/services`), faire démarrer puis arrêter un conteneur de test SANS importance (jamais Terraria) : `docker run -d --name hearth-verif busybox:1.36 sleep 60` puis `docker rm -f hearth-verif`. Attendu : le service apparaît puis disparaît en moins de 5 secondes.
6. **Le moteur tombe.** Non recommandé sur forge (arrêter Docker arrête les jeux). Cette vérification est couverte par la CI (job `engine`) et par les tests ; ne la faire ici que sur décision de Voiky, hors des heures de jeu.
7. **Rôles.** Avec un compte Lecture seule : la même liste, le même état. Aucune entrée dans le journal d'activité pour ces lectures.

À consigner dans la session : version de l'agent, date, résultat de chaque étape, écarts.

## Partie 1 bis : accorder le droit, puis adopter (HRT-79, HRT-80)

Depuis HRT-79 la liste ne montre plus que les services créés par Hearth et les services ADOPTÉS : sur forge, où tous les services sont hors Hearth, l'étape 3 ci-dessus donne d'abord une liste VIDE et « n conteneurs créés hors de Hearth ne sont pas gérés ». **Premier geste : accorder le droit, puis adopter.**

1. **Accorder le droit** à son compte, sur le serveur, en root : `docs/runbooks/accorder-le-droit-de-gerer-les-services.md` (`hearth-agent account services {compte} --grant`). Le droit ne se donne jamais depuis le réseau.
2. **Constater.** `GET /api/v1/services` : `services` vide, `engine.external_count` = le nombre de conteneurs de `docker ps -a`. `GET /api/v1/hello` : `capabilities` contient `service-adoption`.
3. **Lire les candidats** (`GET /api/v1/services/external`, avec le droit) : `terraria`, `7dtd`, `forge-studio`, chacun avec son unité (`docker-terraria.service`…), son image, son état ; la clé d'un service lancé par une unité est `u_<unité>`. Sans le droit : `403`. À consigner : les candidats manquants ou en trop.
4. **Adopter** (jamais Terraria sans autosave ni sauvegarde préalables, règle du hub : adopter n'y touche pas, mais le geste suivant, arrêter, si). Depuis le client quand HRT-81 est livré ; sinon `deploy/e2e/admin_act.py confirm-adopt HOST PORT TOKEN USER PASSWORD KEYFILE` (corps `{"keys": ["u_docker-terraria.service"]}`), qui fournit la preuve de clé et le mot de passe. Attendu : `adopted: 1`, le service apparaît dans la liste de tous les comptes avec « adopté par {compte} », `external_count` baisse d'un.
5. **Vérifier que rien n'a changé.** `docker inspect` du conteneur et `systemctl show docker-terraria.service` avant et après : identiques (étiquettes, état, `MainPID`, `ActiveEnterTimestamp`). L'adoption n'écrit rien sur le service.
6. **Le journal d'activité** (administrateur) : une entrée « Service adopté » par service, cible « service terraria (docker-terraria.service) », jamais un autre détail du conteneur.
7. **Lecture seule.** Un compte sans le droit voit le service adopté dans la liste mais sans commande ni noms de variables (`details_reserved`), et reçoit `MISSING_RIGHT` (consigné) s'il ouvre ses journaux.
8. **Relâcher puis adopter de nouveau** un conteneur de test sans importance (`docker run -d --name hearth-verif busybox:1.36 sleep 600`) : « Ne plus gérer » (`POST /services/{key}/release`) le retire de la liste de tous sans toucher au conteneur ; `docker rm -f hearth-verif` ensuite. Un conteneur SANS unité recréé par un tiers redevient non géré.

À consigner dans la session : version de l'agent, date, résultat de chaque étape, écarts. Ce que la CI seule prouve : le comportement d'un vrai Docker et d'un vrai systemd (`engine_supervised.rs`, étape `supervised`).

## Parties suivantes (squelette)

- Partie 2 (HRT-63) : détail d'un service, variables masquées puis révélées, service géré par une unité systemd (`u_docker-*.service`), ce que fait un arrêt par l'API du moteur sur un conteneur `--rm`.
- Partie 3 (HRT-64) : démarrer, arrêter, redémarrer un service (jamais Terraria sans sauvegarde ; 7 Days to Die : le délai d'arrêt réel de 120 s).
- Partie 4 (HRT-66) : journaux en direct, pilote `journald`.
- Partie 5 (HRT-68, HRT-70) : déployer une image, dossiers de données, retour des services après `nixos-rebuild switch` et après un redémarrage du serveur.

## Diagnostic des journaux

- `RUST_LOG` ne rend jamais bavardes les bibliothèques de transport du moteur : `bollard`, `bollard_stubs`, `hyper`, `hyper_util`, `hyperlocal`, `hyper_named_pipe` et `h2` restent au niveau `warn` pour TOUT l'agent (serveur HTTPS et client de mise à jour compris), quelle que soit la directive (`RUST_LOG=hyper_util=debug` est ignoré). C'est voulu : leurs traces de niveau `debug` contiennent les variables d'environnement des conteneurs (SECURITY.md).
