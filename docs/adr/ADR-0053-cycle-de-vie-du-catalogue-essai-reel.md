---
id: ADR-0053
titre: Cycle de vie du catalogue : une fiche est essayée sur sa vraie image avec un vrai moteur avant d'atteindre un utilisateur ; ce qui n'a pas été vu reste `untested` ou `preview`, et le résultat de l'essai est consigné, jamais supposé
type: architecture
statut: proposée (première moitié posée par HRT-114 : le banc d'essai de la fiche Terraria ; relevé `golden`, veille nocturne et registre sont à HRT-115)
date: 2026-11-14
portee: repo
remplace: le brouillon de l'ADR-0053 du hub (2026-10-13), dont ce texte reprend la première moitié
liens: [ADR-0036, ADR-0050, ADR-0051, ADR-0052, conception technique 2026-10-13 (fiches de jeu) sections 12.2 à 12.5, BR-FICHE-006, BR-FICHE-008, BR-FICHE-028, BR-FICHE-037, BR-JEU-038, BR-RES-038]
---

# ADR-0053 : Cycle de vie du catalogue, l'essai réel d'abord

## Contexte
Une fiche (ADR-0050) est écrite d'après un inventaire de documentation et d'une installation de référence : ses ordres d'arrêt, son signe « prêt », ses chiffres de ressources ne sont pas vus par Hearth. Le format sait le dire (`status = "untested"`, `evidence = "unmeasured"`), et l'agent n'exécute ni ne croit jamais un élément `untested` (BR-JEU-038). Reste à lever ces statuts avec une preuve, sans machine de Voiky ni serveur de jeu à la maison.

## Décision
1. **Le banc d'essai réel est un test d'intégration ignoré par défaut** (`crates/hearth-agent/tests/fiche_live.rs`, `HEARTH_TEST_FICHE_LIVE=1`) qui tourne en CI sur le moteur du runner Linux (`.github/workflows/fiche-live.yml`, job `fiche-live-terraria`). Aucun essai d'image n'est lancé automatiquement sur un poste de développement ; seul l'essai local MANUEL décrit par `docs/runbooks/essayer-une-fiche-en-local.md` est permis (HRT-226, conteneur de test Linux sur Docker Desktop, réservé au poste du propriétaire, jamais sur un runner partagé : le socket du moteur y donne root sur la machine virtuelle).
2. **L'essai prend le chemin de l'agent**, pas un raccourci : la fiche embarquée, copiée avec ses éléments `untested` passés à `tested` EN MÉMOIRE (« l'essai sur une copie ne change qu'un mot »), est traduite (`translate`), jugée par le validateur du déploiement ordinaire (`body_and_secrets_from`), transformée en plan fermé (`plan_from_spec`), créée et pilotée par l'adaptateur réel (`BollardEngine`), avec les ordres littéraux de la fiche (`send_stdin_line`) et l'arrêt du moteur aussitôt après l'ordre de sortie. Les dossiers gérés et le fichier de réglages sont posés comme l'installation le fait (`SafeFs` et le journal des opérations ne sont pas dans la boucle : limite assumée, couverts par leurs propres tests).
3. **Contrôles, liste fermée** : le signe « prêt » dans les lignes écrites après le lancement ; le port publié accepte une connexion TCP ; un réglage est pris en compte (le monde porte le nom posé) et le jeu n'a pas réécrit le fichier de Hearth ; les ordres de la suite arrivent par l'entrée du conteneur SANS terminal (le jeu sort de lui-même, code 0, avant la coupure forcée) ; l'ordre `save` réécrit le fichier du monde ; le service arrêté reste arrêté (`unless-stopped` + arrêt demandé au moteur) ; le monde existe après l'arrêt ; un redémarrage atteint de nouveau le signe sans créer un autre monde ; mesures au repos : mémoire 60 s après le signe, espace de l'image plus des dossiers gérés.
4. **Le résultat est consigné ; il n'est jugé que pour ce que la fiche livrée affirme.** L'essai écrit `evidence.toml` (artefact du job et résumé de l'exécution : empreinte de l'image relevée, chaque constat, mesures). Tant qu'un élément est `untested`, son essai est un RÉSULTAT (un jeu qui ne lit pas son entrée sans terminal laisse la séquence `untested`), pas un échec du job ; une fois l'élément promu à `tested`, le banc l'EXIGE et échoue s'il ne tient plus ; ce qui fait échouer le job est ce que toute fiche doit tenir : démarrer, répondre, prendre un réglage, rester arrêtée, retrouver son monde.
5. **Un statut ne change que par un relevé.** Le passage de `untested` à `tested` dans `fiche.toml` est un commit qui cite le relevé ; `fiche.toml` pose aussi l'empreinte de l'image relevée (`image.digest`), et la version de la fiche augmente (`catalog-check` l'exige quand le contenu change).
6. **Règle de publication inchangée** (conception 12.5, BR-RES-038) : une fiche reste `preview` tant qu'il manque le relevé de référence des réglages (`golden`) ou une mémoire recommandée mesurée sous charge ; `measured_idle` (le plancher au repos) alimente `min_bytes` et `install_bytes`, jamais une recommandation. `catalog-check --release` refuse toujours une fiche `preview`.
7. **Déclenchement à la main seulement** (`workflow_dispatch`) : jamais sur push, pull request ni chemin (décision de Voiky du 2026-11-14 : le quota de minutes GitHub Actions a dépassé 80 % le mois précédent). Il se lance avant de changer le statut d'un élément de fiche, après toute modification de la fiche ou du banc, et avant une release qui embarque une fiche essayée ; `gh workflow run "Banc d'essai des fiches" --ref <branche>`. Écart avec la conception 12.3 (« CI, une tâche par fiche légère »), assumé.

## Résultat du premier essai (14/11/2026, `ryshe/terraria:vanilla-1.4.5.8`)
Point B6 levé : Terraria lit son entrée sans terminal. `save` réécrit le monde ; `exit` fait sortir le jeu de lui-même (code 0, moins d'une seconde) ; le service reste arrêté ; le monde survit au redémarrage. Signe « Server started » vu 21 s après le lancement (petit monde). Mémoire au repos 564 Mio, espace installé 53 014 364 octets (image 50 068 911 + dossiers gérés après un premier démarrage) ; empreinte `sha256:3ece8ceb88c11e47bc490d4e17849e89f5343722142fca506fbde9a7ab04a677`.

## Résultat de l'essai de la fiche Minecraft Java (15/11/2026, forge, HRT-116)
Banc : `crates/hearth-agent/tests/fiche_live_minecraft.rs`, lancé SUR LA FORGE (Linux, Docker natif, 32 Go ; aucun `workflow_dispatch`), binaire de test statique construit dans `rust:1.95-alpine`. Chaque scénario passe par le chemin de l'agent (traduction, jugement du déploiement ordinaire, plan fermé, création par `BollardEngine`) sur la vraie image épinglée de la fiche, sans joueur : signe « prêt », port, réglage pris en compte (`MOTD` dans `server.properties`), arrêt par le SIGNAL de l'image (code 0, `level.dat` réécrit, avant le délai), service qui reste arrêté, redémarrage sur le même monde (jeu officiel), mémoire (cgroupe : pic au démarrage, repos 60 s après le signe) et disque.

**Défaut trouvé et corrigé par l'essai** : « LATEST » prend la version 26.3 du jeu, compilée pour Java 25 : elle ne démarrait pas sur l'image `java21` de la fiche (`UnsupportedClassVersionError`, classe version 69). La fiche passe à l'étiquette `java25` (`sha256:52a530ef...`) pour le jeu officiel, Paper et Purpur ; NeoForge reste sur `java21`, Fabric, Quilt et Forge sur `java17` (packs de la 1.20). Version de la fiche : 2.

| Scénario | Limite | Signe « prêt » | Repos (compteur du moteur) | Pic de démarrage (cgroupe, cache compris, Mo et Go décimaux) | Arrêt |
|---|---|---|---|---|---|
| Jeu officiel 26.3 | 2 Gio | 10 s | 552 Mio | 696 Mo | 0 s, code 0, monde enregistré |
| Jeu officiel 26.3 | 4 Gio | 12 s | 698 Mio | 872 Mo | 0 s, code 0, monde enregistré |
| Fabric 1.20.1 + Prominence II 4.1.8 (377 mods) | 10 Gio | 95 s | 7,6 Gio | 9,4 Go | 3 s, code 0, monde enregistré |

Disque : 1 497 623 985 octets pour le jeu officiel (image 1 363 527 826 + dossiers gérés), 5 248 719 852 avec le pack (fichier de 1 577 339 200 octets). Le redémarrage du jeu officiel retrouve son monde.
Promotion : `ready` passe à `tested` ; mémoire `measured` avec `min_bytes` = 2 Gio et `recommended_bytes` = 4 Gio (les LIMITES essayées : aucun plancher plus bas n'a été essayé, donc aucun n'est affirmé), cran « Ultra » à 10 Gio (le gros pack, essayé) ; `status = "published"`. Le processeur reste non mesuré (non exigé pour publier). `catalog-check --release` ne reproche plus rien à Minecraft (il reproche encore Terraria, `preview`). Limite : aucun joueur, donc aucune charge de jeu ; les chiffres sont ceux d'un serveur vide.

Relevé brut (`evidence.toml`, engendré par le banc) :
```
# Relevé du banc d'essai réel (HRT-116). Engendré par `tests/fiche_live_minecraft.rs`, jamais écrit à la main.
fiche = "minecraft"
fiche_version = 2
vanilla_min.memory_limit_bytes = 2147483648
vanilla_min.image = "itzg/minecraft-server:java25@sha256:52a530ef9f3af25c0536c21e5b7c6bad7c1e1e7777fc186c4196c3d1f7635113"
vanilla_min.image_digest = "itzg/minecraft-server@sha256:52a530ef9f3af25c0536c21e5b7c6bad7c1e1e7777fc186c4196c3d1f7635113"
vanilla_min.disk_image_bytes = 1363527826
vanilla_min.ready = true
vanilla_min.ready_after_s = 10
vanilla_min.memory_peak_startup_bytes = 696377344
vanilla_min.tcp_accepts = true
vanilla_min.setting_taken = true
vanilla_min.memory_usage_raw = "552.4MiB / 2GiB"
vanilla_min.memory_idle_bytes = 719118336
vanilla_min.memory_peak_bytes = 722309120
vanilla_min.disk_install_bytes = 1497603780
vanilla_min.stop_took_s = 0
vanilla_min.stop_exit_code = "0"
vanilla_min.world_saved_on_stop = true
vanilla_min.world_saved_in_logs = true
vanilla_min.stop_clean = true
vanilla_min.no_auto_restart = true
vanilla_min.restart_ready = true
vanilla_min.restart_keeps_world = true
vanilla_rec.memory_limit_bytes = 4294967296
vanilla_rec.image = "itzg/minecraft-server:java25@sha256:52a530ef9f3af25c0536c21e5b7c6bad7c1e1e7777fc186c4196c3d1f7635113"
vanilla_rec.image_digest = "itzg/minecraft-server@sha256:52a530ef9f3af25c0536c21e5b7c6bad7c1e1e7777fc186c4196c3d1f7635113"
vanilla_rec.disk_image_bytes = 1363531922
vanilla_rec.ready = true
vanilla_rec.ready_after_s = 12
vanilla_rec.memory_peak_startup_bytes = 872296448
vanilla_rec.tcp_accepts = true
vanilla_rec.setting_taken = true
vanilla_rec.memory_usage_raw = "698.5MiB / 4GiB"
vanilla_rec.memory_idle_bytes = 872181760
vanilla_rec.memory_peak_bytes = 875819008
vanilla_rec.disk_install_bytes = 1497623985
vanilla_rec.stop_took_s = 0
vanilla_rec.stop_exit_code = "0"
vanilla_rec.world_saved_on_stop = true
vanilla_rec.world_saved_in_logs = true
vanilla_rec.stop_clean = true
vanilla_rec.no_auto_restart = true
pack.memory_limit_bytes = 10737418240
pack.image = "itzg/minecraft-server:java17@sha256:94b3e70f40c9ddc8ded7de2a7e7d6725254f08abaa63950d8402e4b107ccc4e2"
pack.image_digest = "itzg/minecraft-server@sha256:94b3e70f40c9ddc8ded7de2a7e7d6725254f08abaa63950d8402e4b107ccc4e2"
pack.disk_image_bytes = 1239052980
pack.pack_bytes = 1577339200
pack.ready = true
pack.ready_after_s = 95
pack.memory_peak_startup_bytes = 9407426560
pack.tcp_accepts = true
pack.setting_taken = true
pack.memory_usage_raw = "7.635GiB / 10GiB"
pack.memory_idle_bytes = 10582118400
pack.memory_peak_bytes = 10585022464
pack.disk_install_bytes = 5248719852
pack.stop_took_s = 3
pack.stop_exit_code = "0"
pack.world_saved_on_stop = true
pack.world_saved_in_logs = true
pack.stop_clean = true
pack.no_auto_restart = true
```

## Limites
- Un petit monde et aucun joueur : ni charge, ni grand monde, ni reprise d'un monde modifié par un joueur (cela reste à la forge).
- Le fichier du monde est relu par sa taille et sa date, pas par un client de jeu. L'effet de `save` est jugé sur un relevé pris AVANT `exit` (qui enregistre aussi).
- Une fois un statut promu dans la fiche livrée, l'essai l'EXIGE : si l'image ne lit plus son entrée, le job échoue.
- Pas de registre d'empreintes ni de veille nocturne contre l'étiquette la plus récente (HRT-115), pas de relevé `golden` ni de `probe.toml` déclaratif : l'essai est écrit en Rust pour la seule fiche Terraria ; il se généralisera avec la deuxième fiche.

## Rejeu local et publication de Terraria (16/11/2026, HRT-226)
Banc : `tests/fiche_live.rs`, rejoué EN LOCAL sur Docker Desktop (WSL2, 8 Gio) depuis un conteneur Linux `rust:1.95` qui monte le socket du moteur (aucun `workflow_dispatch`, quota Actions). Les dossiers gérés sont posés dans un volume nommé monté au MÊME chemin dans le conteneur de test et dans le moteur (un bind-mount est résolu par le moteur, pas par le conteneur de test). Résultat sur `ryshe/terraria:vanilla-1.4.5.8` (`sha256:3ece8ceb...`) : signe « prêt » en 14 s, port TCP répond, monde nommé comme demandé, `save` puis `exit` lus sans terminal (code 0, 0 s), service qui reste arrêté, monde conservé au redémarrage ; mémoire 593 Mio au repos avec la limite de 2 Gio. Promotion : `status = "published"`, `memory` = 2 Gio `measured` (la limite essayée : aucun plancher plus bas affirmé), fiche en version 5, `evidence.toml` et `golden.toml` (relevé `docker image inspect` de l'image épinglée) à côté de la fiche.

## Banc générique (HRT-226)
`tests/fiche_live_game.rs` généralise le banc de Minecraft aux jeux à arrêt par signal : une entrée du tableau `GAMES` par jeu, un relevé `<out>/<id>/evidence.toml`. Il révèle des défauts de fiche que la lecture ne montre pas : voir les tickets HRT-227 et suivants (7 Days to Die ne démarre pas sous `no-new-privileges`).

## Les autres jeux (16/11/2026, HRT-115, HRT-227 à HRT-235)
Banc générique `tests/fiche_live_game.rs` lancé en local (Docker Desktop, machine virtuelle de 8 Gio et 62 Go de disque, tous les conteneurs sous `no-new-privileges`). Résultat par jeu (chaque ligne : image épinglée, ce que le banc a vu, statut) :

| Jeu | Image | Résultat | Statut |
|---|---|---|---|
| Terraria | `ryshe/terraria:vanilla-1.4.5.8` | banc spécifique (HRT-114) rejoué : tout tient | publiée (v5) |
| Valheim | `lloesche/valheim-server:sha-0ab6cdcc16dd` | prêt 158 s, arrêt code 0 en 8 s, 1,57 Gio au repos | publiée |
| Project Zomboid | `danixu86/project-zomboid-dedicated-server:42.20.4-release-2` | prêt 74 s, arrêt code 0 en 23 à 71 s, 2,9 Gio au repos | publiée |
| 7 Days to Die | `vinanrra/7dtd-server:v0.10.0` | le conteneur ne démarre pas sous `no-new-privileges` (`crontab` setgid) | preview (HRT-227) |
| Factorio | `factoriotools/factorio:2.1.24` | tout tient (405 Mio au repos) mais le JSON de réglages n'est pas lu | preview (HRT-229) |
| Core Keeper | `escaping/core-keeper-dedicated:v2.8.1` | arrêt en code 143 | preview (HRT-230) |
| V Rising | `trueosiris/vrising:2026-04-23-1036` | l'arrêt (code 0, 2 s) ne réécrit aucune sauvegarde | preview (HRT-232) |
| Enshrouded | `mornedhels/enshrouded-server:1.8.1-wine` | prêt 166 à 207 s, arrêt code 0 en 78 s, 0,8 à 0,9 Gio au repos (serveur vide) | publiée (HRT-235) |
| Palworld, Satisfactory | README + essai manuel (pas le banc) | démarrent ; arrêt en code 143 et 130 | preview (HRT-233, 234) |

Défauts de format ou de banc révélés : les noms de variables d'environnement d'une fiche sont en majuscules seulement (V Rising écrit en casse mixte) ; une image dont le premier démarrage sort puis repart (SteamCMD qui se met à jour : V Rising, Satisfactory) est rejouée par `unless-stopped` et ses variables ne s'appliquent qu'au second passage ; un arrêt par signal qui sort en code 143 ou 130 après avoir enregistré (Core Keeper, Palworld, Satisfactory) est refusé par la règle du code 0 : à trancher.
