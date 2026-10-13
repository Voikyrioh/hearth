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

Le moteur de conteneurs (note provisoire, HRT-61, finalisée avec HRT-70) : l'agent lit le moteur de conteneurs du serveur par le socket Unix du moteur (ADR-0036). **Le socket d'un moteur de conteneurs équivaut à `root`** : qui le pilote monte `/` dans un conteneur. La tranche actuelle ne fait que lire (détection, liste des conteneurs, événements, unités systemd) ; les tranches suivantes ajouteront démarrer, arrêter et déployer, et un administrateur Hearth deviendra alors, en pratique, `root` du serveur. L'administrateur du serveur peut couper tout cela : `[engine] enabled = false` dans `agent.toml` (ou `HEARTH_ENGINE=off`) : l'agent n'ouvre alors jamais le socket du moteur.

Variables d'environnement d'un service (HRT-63, BR-SVC-008, BR-SVC-009) : la liste et le détail portent les NOMS des variables et la commande de lancement pour les deux rôles (la spec le veut ainsi) ; **une commande peut contenir un secret passé en argument (`--token=...`) et un compte en lecture seule la voit**. Les VALEURS ne voyagent jamais dans la liste, le détail ni le flux : `GET /services/{key}/env` les rend aux administrateurs seulement (refus d'un compte en lecture seule et réussite consignés au journal d'activité, sous `service.env.reveal`, sans nom ni valeur ; réponse `Cache-Control: no-store`). Côté agent un nom ou un argument est un `Hidden` et une valeur un `Secret` (`Debug` masqué, valeur effacée de la mémoire) ; sur le fil, `Concealed` (`Debug` masqué). Ils n'entrent dans aucune trace, aucun journal, aucune table, aucun message d'erreur : `tests/services_https.rs` le vérifie au niveau de trace le plus bavard, sur la révélation en réussite et en refus. Les messages WebSocket portent maintenant ces noms et cette commande : la crate `tungstenite` (qui écrit chaque message au niveau `trace`) est ajoutée aux cibles gardées du filtre de traces ci-dessous.

Lecture de `/proc` et unités systemd (HRT-63, ADR-0037) : pour savoir quelle unité lance un conteneur, l'agent lit en lecture seule `/proc/<pid>/{exe,cmdline,cgroup}` des clients `docker` et `podman` (au plus 100 000 processus et 2 s par recherche, une fois par conteneur) et lance `systemctl show` (jamais par un interpréteur, un argument à la fois, après `--`, délai de 5 s). Un nom d'unité vient du système, donc n'est pas de confiance : il est validé (jeu de caractères d'un nom d'unité, 255 caractères, suffixe `.service`) avant d'entrer dans une clé de service, un argument de commande ou la base. Un processus qui n'est pas un client du moteur ne désigne aucun conteneur, même si sa ligne de commande imite un `docker run`. Hearth n'agit sur aucune unité dans cette tranche (ni démarrer, ni arrêter, ni `disable`, ni `mask`, ni écriture d'unité).

Journaux et moteur de conteneurs (HRT-61, revue de la PR 77) : la bibliothèque `bollard` écrit, au niveau `debug` de la crate `log`, le corps de CHAQUE réponse décodée, donc, à chaque lecture d'un conteneur, ses variables d'environnement (valeurs comprises), sa commande et ses étiquettes. Hearth n'écrit rien de tel, mais `RUST_LOG=debug` aurait mis ces valeurs dans le journal de l'agent. Garde en deux étages : (1) `infrastructure/logging.rs::TransportGuard` est un filtre de cible, ajouté APRÈS `RUST_LOG` et que nulle directive ne dépasse (même `bollard::docker=trace`) : `bollard`, `bollard_stubs`, `hyper`, `hyper_util`, `hyperlocal`, `hyper_named_pipe`, `h2`, `tungstenite` et `tokio_tungstenite` ne sortent jamais sous le niveau `warn`, y compris par le pont `log` vers `tracing` (il lit la vraie cible des traces du pont) ; (2) le test `tests/services_https.rs::no_variable_value_reaches_a_message_a_response_or_a_trace_and_no_name_or_command_reaches_a_trace` installe le journal par le MÊME chemin que la production (`logging::try_init_with`, pont `log` compris), au niveau `trace`, avec une directive qui vise `bollard`, fait lire par le vrai adaptateur un conteneur dont l'inspection porte une valeur sentinelle dans `Env` et dans `Cmd`, et cherche la sentinelle dans la sortie capturée (il échoue quand on retire le filtre). Limite : seules ces cibles sont gardées ; les autres dépendances (`reqwest`, `sqlx` qui écrit le texte des requêtes SQL au niveau `debug`, `rustls`) n'ont pas été auditées une à une pour cette tranche. À chaque montée de version de `bollard` (0.21.1 aujourd'hui, avec `http` et `pipe`), revérifier que ses traces `warn` et `error` ne citent aucune valeur de conteneur : elles sont aujourd'hui toutes sous `grpc/`, hors des fonctionnalités activées. Le filtre annonce un niveau maximal `trace` (`max_level_hint`) pour ne pas forcer le niveau global ; le plafond reste celui de `RUST_LOG` (`tests/logging_level_hint.rs`). Tout ce qui vient d'un conteneur (nom, image, étiquette, chemins, nombre de ports et de montages) est aussi nettoyé des caractères de contrôle et plafonné avant d'aller sur le fil (`domain/services/limits.rs`, BR-SVC-021).

Journaux d'un service (HRT-66, BR-SLOG-018, BR-SLOG-019) : **aujourd'hui, tout compte authentifié, lecture seule comprise, lit les journaux de tous les conteneurs du serveur que la liste des services montre** (les deux rôles, services adoptés ou non) ; le droit de gérer les services et l'adoption restreindront cette lecture par d'autres tickets (HRT-76, HRT-79). Un programme écrit ce qu'il veut dans ses journaux, secrets compris. **Les lignes d'un service sont du contenu sensible** : l'agent ne les analyse pas, ne les masque pas, ne les garde pas, ne les trace pas. Elles passent par l'anneau borné d'une lecture (5 000 lignes, 2 Mio, dix lectures au plus) et meurent avec elle ; aucune n'entre dans une trace de l'agent à aucun niveau (test sentinelle par le chemin d'initialisation du journal de la production, au niveau le plus bavard, avec une directive qui vise `bollard`, `hyper` et `tungstenite`), ni dans la base, le journal d'activité ou la file des opérations ; les types qui les portent cachent leur texte dans `Debug`. Tout ce qui vient du programme est borné (une ligne en cours d'assemblage : 64 Kio ; le texte envoyé : 10 000 caractères ; un lot : 500 lignes et 256 Kio ; une trame du moteur : 256 Kio ; une lecture : 2 Mio d'anneau ; dix lectures) et nettoyé avant d'être envoyé (codes de couleur et séquences du terminal, caractères de contrôle, caractères Unicode invisibles ou qui renversent l'écriture : `hearth_proto::log_text::clean`). La lecture des journaux est écrite à la main sur le socket du moteur (`infrastructure/engine/logs.rs`) et non par `bollard::Docker::logs`, pour deux raisons lues dans le décodeur de `bollard` 0.21.1 : une ligne de terminal qui ne se termine pas s'y accumule sans borne (rien n'est rendu avant une fin de ligne), et à la fin du flux les octets restants sont écrits dans une trace de niveau `debug`, c'est-à-dire du contenu de journal dans une trace. (Le même décodeur prend aussi pour un en-tête de huit octets un tampon qui commence par 0, 1 ou 2 ; avec les horodatages que l'agent demande toujours, chaque message du moteur commence par un chiffre, ce chemin n'est donc pas atteignable et ce n'est pas la raison du remplacement.) Une lecture sans suivi qui reste muette plus de 30 s est abandonnée. Une reprise (`open { since }`) ne fait jamais relire un journal entier : le curseur du client n'est pas cru sur parole (plus vieux que 5 minutes, dans le futur, négatif ou au rang absurde : reprise perdue, soit un trou puis les 1 000 dernières lignes ; la relecture qui vise un curseur est plafonnée à 20 000 lignes sautées et le moteur ne rend jamais plus de 25 000 messages pour une requête qui porte `since` ; un conteneur recréé n'est jamais lu « depuis son début » au-delà de 5 000 lignes). La décision « ce compte lit ce service » est une seule fonction du domaine (`domain::services::logs::may_read`, qui reçoit le compte et l'origine du service), appelée à l'ouverture, à chaque résolution du conteneur, à chaque changement du compte vu par la session et périodiquement (2 s) pendant la lecture : un droit retiré ou une session fermée met fin à la lecture ; l'adaptateur du moteur ne connaît ni rôle ni compte. Lire n'est pas journalisé (BR-SLOG-022). Le socket est authentifié par son premier message comme `/stream` (mêmes quotas de connexions en attente d'authentification) ; sa place d'attente est gardée jusqu'à l'`open` (un compte authentifié qui n'ouvre rien ne multiplie pas ses sockets) puis rendue, et la lecture est comptée par ses dix places.

La confiance repose sur trois choses, aucune n'est dans ce dépôt : la clé de signature des mises à jour, l'empreinte de chaque serveur, et les mots de passe. Le serveur qui distribue les versions n'est pas de confiance : c'est la signature qui fait foi.
