---
id: ADR-0012
titre: Service système : root, unité systemd durcie, installation gérée sans unité
type: securite
statut: acceptée
date: 2026-10-05
portee: projet
remplace: —
liens: [ADR-0003, ADR-0008, ADR-0009, conception technique 2026-10-04 sections 2 et 7, HRT-15]
---

# ADR-0012 : Service système

## Contexte

`hearth-agent install` doit installer l'agent comme un service qui démarre avec le serveur (BR-INSTALL-005). Deux questions : sous quel utilisateur tourne-t-il, et que fait l'agent quand le système gère déjà ses services (NixOS) ?

La suite du produit demande à l'agent de piloter Docker (socket `/var/run/docker.sock`), l'alimentation de la machine (arrêt, redémarrage) et de se mettre à jour lui-même (HRT-17 : échange du binaire dans `/usr/local/bin`, redémarrage par le gestionnaire de services).

## Décision

1. **L'agent tourne en root**, dans une unité systemd durcie. Un utilisateur dédié (`hearth`) ne protégerait presque rien : pour piloter Docker il lui faudrait l'appartenance au groupe `docker` (équivalent à root : monter `/` dans un conteneur suffit), pour éteindre la machine une règle polkit, pour se mettre à jour la propriété de son propre binaire (donc la possibilité de le remplacer par un autre). Le gain apparent coûterait trois mécanismes à maintenir, sans barrière réelle.
2. **Le durcissement est dans l'unité** (`infrastructure/service/systemd.rs::render_unit`) :
   - `Restart=always`, `RestartSec=5`, `WantedBy=multi-user.target`, `After=network-online.target`.
   - `NoNewPrivileges=yes`, `ProtectSystem=full` (`/usr`, `/boot`, `/etc` en lecture seule) avec `ReadWritePaths=` limité au dossier de données et au dossier du binaire (mise à jour), `ProtectHome=read-only`, `PrivateTmp=yes`.
   - `ProtectKernelModules`, `ProtectKernelLogs`, `ProtectControlGroups`, `ProtectClock`, `RestrictSUIDSGID`, `RestrictRealtime`, `LockPersonality`, `SystemCallArchitectures=native`.
   - `UMask=0077` : tout fichier créé par l'agent n'est lisible que par root.
   - **Pas** de `MemoryDenyWriteExecute` (hérité par `nvidia-smi`, qui y échoue), pas de `ProtectKernelTunables` ni de restriction d'espaces de noms (inutiles et à revoir avec Docker).
3. **L'unité ne contient aucun secret** : ni mot de passe, ni haché, ni variable d'environnement. L'agent lit tout dans sa base et son dossier de données. Les chemins écrits dans l'unité sont validés (absolus, caractères sûrs) : aucun chemin ne peut y injecter une directive.
4. **Installation gérée** (`--managed`, ou `HEARTH_MANAGED=1`) : adaptateur `none` (`Unmanaged`) : l'agent n'écrit aucune unité, ne copie pas son binaire, ne lance rien, et le dit. Il crée le dossier de données, l'identité, le premier compte et une configuration `managed = true` (donc pas de mise à jour automatique). Le système déclare et démarre le service. Sans systemd et sans `--managed`, l'installation refuse et suggère `--managed`.
5. **Chemins fixes** : `/usr/local/bin/hearth-agent`, `/etc/hearth/agent.toml`, `/var/lib/hearth/`, `/etc/systemd/system/hearth-agent.service`. L'unité est écrite par un fichier voisin puis un renommage (jamais d'unité à moitié écrite), droits 0644.

## Comment l'appliquer

- Port `ServiceManager` (`application/ports/service_manager.rs`), adaptateurs `Systemd` et `Unmanaged` (`infrastructure/service/`). `systemctl` est lancé avec une liste d'arguments, jamais par un interpréteur de commandes.
- Tester l'adaptateur contre un faux `systemctl` (script enregistreur), jamais contre le systemd de la machine de développement. Le vrai systemd est dans `cargo xtask e2e-install` (conteneur Debian).

## Quand NE PAS l'appliquer / limites

- Si l'agent n'a plus besoin de Docker ni de l'alimentation, passer à un utilisateur dédié avec `AmbientCapabilities` minimales serait préférable : revoir cette décision à ce moment.
- **HRT-17 (mise à jour)** doit tenir compte de `ProtectSystem=full` (le dossier du binaire est écrivable, pas `/etc`), de `PrivateTmp` (un fichier déposé dans `/tmp` n'est pas visible hors de l'unité : utiliser le dossier de données) et du fait que `systemctl stop` tue tout le groupe de contrôle de l'unité, y compris un superviseur lancé « détaché » (le lancer par `systemd-run`, ou régler `KillMode=process`).
- Un système avec un autre gestionnaire que systemd (OpenRC, runit) : `--managed`, l'agent n'écrit rien.

## Alternatives rejetées

- **Utilisateur de service dédié** : voir la décision 1.
- **Unité de service utilisateur** (`systemctl --user`) : ne démarre pas au boot sans `loginctl enable-linger`, et ne peut pas piloter l'alimentation.
- **Écrire un script d'init** : réservé aux systèmes sans systemd, hors périmètre.
- **Variables d'environnement dans l'unité** (`Environment=HEARTH_ADMIN_PASSWORD=…`) : un secret lisible de tous les utilisateurs par `systemctl show`.

## Conséquences

- L'installation est atomique côté unité et réversible (BR-INSTALL-008).
- La compromission du processus agent donne root, atténuée par le bac à sable de l'unité : la surface exposée est celle de l'API HTTPS authentifiée (ADR-0004, ADR-0005).

## Références

- `systemd.exec(5)`, `systemd.service(5)`.
- ADR-0003 (binaire statique), ADR-0008 (mises à jour signées).
