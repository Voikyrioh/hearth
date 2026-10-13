---
id: ADR-0037
titre: Un service a une clé stable et un pilote (moteur ou superviseur systemd détecté) ; le moteur est la vérité, la base de l'agent ne retient que ce que lui seul sait
type: architecture
statut: proposée (non validée par Voiky ; posée avec HRT-61)
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
- **HRT-63** : superviseur `proc_systemd`, clé `u_`, table `supervised_services` (migration `0009`).

## Alternatives rejetées
- **Piloter par l'API du moteur et annoncer « reparti tout seul »** : aucun arrêt ne tiendrait sur forge, aucun démarrage possible, sauvegarde d'un serveur de jeu coupée.
- **Reconnaître les unités par leur nom (`docker-*.service`)** : propre à NixOS.
- **Identité par nom de conteneur** : un conteneur supprimé puis recréé par un tiers sous le même nom serait confondu (règle d'identité stable de la spec « voir les services », HRT-63).
- **État voulu tenu dans la base de l'agent et réconcilié au démarrage** : deux vérités, secrets en base ; à reconsidérer quand un service devra être recréé (mise à jour, duplication).

## Conséquences
- Plusieurs règles des specs « voir les services » et « cycle de vie » sont à amender : la liste, règle par règle, est dans la section « Écarts avec les specs fonctionnelles » de la conception technique.
- Migration `0009_services.sql`.
- Un test CI avec un vrai systemd et un vrai moteur (`systemd-run` d'un `docker run --rm`).

## Références
- nixpkgs, `nixos/modules/virtualisation/oci-containers.nix`.
- `forge-nixos` : `docker.nix`, `jeux/terraria.nix`, `jeux/7dtd.nix`.
- `systemd.service(5)`, `systemctl(1)`.
