# Politique de sécurité

Hearth pilote un serveur : une faille ici peut donner la main sur une machine. Si tu en trouves une, merci de la signaler en privé plutôt que dans une issue publique.

*English summary: please report vulnerabilities privately through GitHub (Security tab, "Report a vulnerability"), not in a public issue. Details below, in French.*

## Signaler une faille

1. Ouvre l'onglet **Security** de ce dépôt, puis **Report a vulnerability**. Le signalement n'est visible que du mainteneur et de toi.
2. Décris ce que tu as trouvé :
   - la version de Hearth (client et agent) et le système du serveur ;
   - ce qu'un attaquant peut faire, et depuis où (même réseau local, autre compte du serveur, serveur de mise à jour, poste Windows) ;
   - les étapes pour le reproduire, ou une preuve de concept ;
   - si tu en as une, une idée de correction.

N'ouvre pas d'issue publique et n'en parle pas publiquement avant qu'un correctif soit disponible.

## Ce à quoi tu peux t'attendre

Hearth est maintenu par une seule personne, sur son temps libre. Il n'y a ni prime ni délai garanti. L'intention :

- un accusé de réception sous une semaine ;
- un premier avis (confirmé, à creuser, hors périmètre) sous deux semaines ;
- un correctif publié dès qu'il est prêt, avec ton nom dans les notes de version si tu le souhaites.

## Versions concernées

Seule la dernière version publiée reçoit des correctifs. Hearth ne propose jamais de revenir à une version plus ancienne : une faille se corrige par une nouvelle version.

Aucune version n'est encore publiée à ce jour ; la première sera la 0.1.0.

## Ce qui compte comme une faille

- Contourner la connexion, les rôles ou la confirmation des actions d'administration de l'agent.
- Faire accepter par le client ou par l'agent une mise à jour qui n'est pas signée par la clé du projet, ou une version plus ancienne que celle installée.
- Faire accepter au client un serveur dont l'empreinte a changé sans que l'utilisateur le voie.
- Lire ou modifier les comptes, le journal ou les secrets de l'agent sans en avoir le droit.
- Obtenir des droits sur le serveur ou sur le poste Windows par l'installation, la mise à jour ou la désinstallation.
- Faire fuiter un mot de passe, une clé ou une adresse dans un journal, un fichier ou une requête.

## Ce qui n'en est pas une

- Ce que peut faire quelqu'un qui est déjà administrateur du serveur (`root`) ou du poste Windows.
- Un déni de service depuis le réseau local contre son propre serveur.
- L'avertissement Windows à l'installation : l'installateur n'est pas signé par un certificat Windows. Les mises à jour, elles, sont signées et vérifiées.
- Un problème dans une dépendance sans effet démontré sur Hearth : signale-le de préférence au projet concerné.

## Comment Hearth se protège

Pour savoir où chercher, les décisions de sécurité sont écrites dans `docs/adr/` et les règles dans `docs/business-rules/` (connexion, postes de confiance, mode attaque, mises à jour signées, installation). `ARCHITECTURE.md` donne la carte du code.

Le moteur de conteneurs (HRT-61, HRT-76, ADR-0036, ADR-0041) : l'agent lit le moteur de conteneurs du serveur par le socket Unix du moteur. **Le socket d'un moteur de conteneurs équivaut à `root`** : qui le pilote monte `/` dans un conteneur. Le processus de l'agent, lui, tourne en `root` (ADR-0012) et garde le socket : qui prend ce processus a tout. Le rôle d'un compte (Administrateur ou Lecture seule) ne donne donc **aucun** pouvoir sur les services. Un droit distinct, « gérer les services », est porté par le compte, indépendamment de son rôle ; **personne ne l'a par défaut** (migration `0010` : aucun compte existant, pas même le premier administrateur) et **aucune route réseau ne le donne ni ne le retire** : il se donne depuis le serveur seulement, par son administrateur : à l'installation (une question posée pour le premier compte quand un moteur est détecté ; **sans terminal ou avec `--yes`, la réponse vaut non**, `--services` l'accorde explicitement) ou plus tard par la commande `hearth-agent account services {compte} --grant | --revoke`, qui exige `root`, refuse un compte inconnu et consigne chaque octroi ou retrait effectif au journal d'activité (origine « ligne de commande du serveur », jamais par une route ; HRT-77, BR-GRANT-006 à 009). Les comptes existants à la mise à jour de l'agent n'ont pas le droit : l'administrateur du serveur le leur donne un à un. **Le droit ne se prend pas non plus par le réseau** (défaut à confirmer par le propriétaire, BR-GRANT-005) : un administrateur sans le droit pourrait sinon définir le mot de passe d'un compte qui l'a, ouvrir une session à son nom et obtenir l'effet du droit. Un compte qui porte le droit ne voit donc ses moyens de connexion (son mot de passe) modifiés par le réseau que par lui-même ou par un compte qui porte lui aussi le droit ; sinon l'acte est refusé et consigné, et seule la ligne de commande du serveur (`account passwd`) passe. Les autres actes d'un administrateur sans le droit sur un titulaire (changer son rôle, fermer ses sessions, supprimer son compte) peuvent le gêner, jamais le lui prendre : la suppression emporte le droit avec le compte, et un compte recréé sous le même nom naît sans le droit. **La vraie frontière locale** : qui peut écrire le fichier de la base de l'agent peut se donner le droit ; le dossier de données (0700) et la base (0600) sont à `root`, et le contrôle « root » de la commande n'est qu'un message clair, pas la barrière. Le serveur vérifie ce droit à chaque requête d'un acte de service, même si le client est modifié (`Access::Services`) ; un retrait prend effet à la requête suivante. **Ce que le droit donne** : un compte qui a le droit choisit des images et les fait tourner sur le serveur. Il n'obtient pas les pouvoirs de l'administrateur du serveur par ce que Hearth crée, sauf si le serveur a ouvert un dossier sensible aux services, sauf faille du moteur ou du système, et sauf pour un service adopté dont la configuration le permet déjà ; le processus de l'agent, lui, garde les pouvoirs de l'administrateur du serveur. **Ce que voit un compte sans le droit, même en lecture seule** : la liste, les états, les ports, les montages, les NOMS des variables et la commande de lancement de TOUS les conteneurs du serveur ; seules les VALEURS des variables exigent le droit (la restriction de ce qui est publié vient avec l'adoption des services, ADR-0043). Un compte en lecture seule qui a le droit obtient, lui, ces valeurs, donc les identifiants d'autres systèmes qu'elles contiennent, et rien d'autre dans Hearth. **État actuel (HRT-76)** : seules la détection, la liste, le détail et la révélation des valeurs de variables existent ; la révélation exige le droit (refus et réussites consignés au journal d'activité). Le plan fermé du conteneur créé et la liste d'autorisation des dossiers qui bornent ce que le droit permet (ADR-0041, points 5 et 6) sont livrés par des tickets suivants : tant qu'ils ne le sont pas, un compte qui aurait le droit de déployer serait, en pratique, `root` du serveur, et Hearth ne présente jamais le droit comme sans risque (BR-GRANT-022). L'administrateur du serveur peut tout couper : `[engine] enabled = false` dans `agent.toml` (ou `HEARTH_ENGINE=off`) : l'agent n'ouvre alors jamais le socket du moteur, et ce coupe-circuit l'emporte sur le droit.

Variables d'environnement d'un service (HRT-63, BR-SVC-008, BR-SVC-009) : la liste et le détail portent les NOMS des variables et la commande de lancement pour les deux rôles (la spec le veut ainsi) ; **une commande peut contenir un secret passé en argument (`--token=...`) et un compte en lecture seule la voit**. Les VALEURS ne voyagent jamais dans la liste, le détail ni le flux : `GET /services/{key}/env` les rend aux comptes qui ont le droit « gérer les services » seulement, quel que soit leur rôle (un administrateur sans le droit est refusé, un compte en lecture seule qui l'a est servi ; refus et réussite consignés au journal d'activité, sous `service.env.reveal`, sans nom ni valeur ; réponse `Cache-Control: no-store`). Côté agent un nom ou un argument est un `Hidden` et une valeur un `Secret` (`Debug` masqué, valeur effacée de la mémoire) ; sur le fil, `Concealed` (`Debug` masqué). Ils n'entrent dans aucune trace, aucun journal, aucune table, aucun message d'erreur : `tests/services_https.rs` le vérifie au niveau de trace le plus bavard, sur la révélation en réussite et en refus. Les messages WebSocket portent maintenant ces noms et cette commande : la crate `tungstenite` (qui écrit chaque message au niveau `trace`) est ajoutée aux cibles gardées du filtre de traces ci-dessous.

Lecture de `/proc` et unités systemd (HRT-63, ADR-0037) : pour savoir quelle unité lance un conteneur, l'agent lit en lecture seule `/proc/<pid>/{exe,cmdline,cgroup}` des clients `docker` et `podman` (au plus 100 000 processus et 2 s par recherche, une fois par conteneur) et lance `systemctl show` (jamais par un interpréteur, un argument à la fois, après `--`, délai de 5 s). Un nom d'unité vient du système, donc n'est pas de confiance : il est validé (jeu de caractères d'un nom d'unité, 255 caractères, suffixe `.service`) avant d'entrer dans une clé de service, un argument de commande ou la base. Un processus qui n'est pas un client du moteur ne désigne aucun conteneur, même si sa ligne de commande imite un `docker run`. Hearth n'agit sur aucune unité dans cette tranche (ni démarrer, ni arrêter, ni `disable`, ni `mask`, ni écriture d'unité).

Journaux et moteur de conteneurs (HRT-61, revue de la PR 77) : la bibliothèque `bollard` écrit, au niveau `debug` de la crate `log`, le corps de CHAQUE réponse décodée, donc, à chaque lecture d'un conteneur, ses variables d'environnement (valeurs comprises), sa commande et ses étiquettes. Hearth n'écrit rien de tel, mais `RUST_LOG=debug` aurait mis ces valeurs dans le journal de l'agent. Garde en deux étages : (1) `infrastructure/logging.rs::TransportGuard` est un filtre de cible, ajouté APRÈS `RUST_LOG` et que nulle directive ne dépasse (même `bollard::docker=trace`) : `bollard`, `bollard_stubs`, `hyper`, `hyper_util`, `hyperlocal`, `hyper_named_pipe`, `h2`, `tungstenite` et `tokio_tungstenite` ne sortent jamais sous le niveau `warn`, y compris par le pont `log` vers `tracing` (il lit la vraie cible des traces du pont) ; (2) le test `tests/services_https.rs::no_variable_value_reaches_a_message_a_response_or_a_trace_and_no_name_or_command_reaches_a_trace` installe le journal par le MÊME chemin que la production (`logging::try_init_with`, pont `log` compris), au niveau `trace`, avec une directive qui vise `bollard`, fait lire par le vrai adaptateur un conteneur dont l'inspection porte une valeur sentinelle dans `Env` et dans `Cmd`, et cherche la sentinelle dans la sortie capturée (il échoue quand on retire le filtre). Limite : seules ces cibles sont gardées ; les autres dépendances (`reqwest`, `sqlx` qui écrit le texte des requêtes SQL au niveau `debug`, `rustls`) n'ont pas été auditées une à une pour cette tranche. À chaque montée de version de `bollard` (0.21.1 aujourd'hui, avec `http` et `pipe`), revérifier que ses traces `warn` et `error` ne citent aucune valeur de conteneur : elles sont aujourd'hui toutes sous `grpc/`, hors des fonctionnalités activées. Le filtre annonce un niveau maximal `trace` (`max_level_hint`) pour ne pas forcer le niveau global ; le plafond reste celui de `RUST_LOG` (`tests/logging_level_hint.rs`). Tout ce qui vient d'un conteneur (nom, image, étiquette, chemins, nombre de ports et de montages) est aussi nettoyé des caractères de contrôle et plafonné avant d'aller sur le fil (`domain/services/limits.rs`, BR-SVC-021).

La confiance repose sur trois choses, aucune n'est dans ce dépôt : la clé de signature des mises à jour, l'empreinte de chaque serveur, et les mots de passe. Le serveur qui distribue les versions n'est pas de confiance : c'est la signature qui fait foi.
