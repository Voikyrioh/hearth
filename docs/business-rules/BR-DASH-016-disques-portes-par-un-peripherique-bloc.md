---
id: BR-DASH-016
domaine: DASH
titre: Seuls les systèmes de fichiers portés par un périphérique bloc réel sont des disques
statut: active
invariant: true
source: premier smoke de Voiky sur la forge (2026-10-08, ticket HRT-47, constat S1a), BR-DASH-012
maj: 2026-10-08
---

# BR-DASH-016 : Seuls les systèmes de fichiers portés par un périphérique bloc réel sont des disques

## Règle
La liste des disques (identité de la machine, échantillons, carte « Disque le plus plein », historique d'une heure) ne rend que les systèmes de fichiers portés par un **périphérique bloc réel**.

- Sous Linux et macOS, le système donne le périphérique de chaque montage : il doit être un chemin `/dev/…` (`/dev/sda2`, `/dev/mapper/vg-data`, `/dev/nvme0n1p2`). Un jeu de données ZFS (`tank/home`) n'a pas de chemin `/dev` et en est un.
- Exclus : `overlay` et `overlayfs` (conteneurs Docker et containerd, mêmes tailles que `/`), `tmpfs`, `devtmpfs`, `ramfs`, `squashfs`, `proc`, `sysfs`, `cgroup`, autres pseudo-systèmes (BR-DASH-012), partages réseau (`nas:/export`), FUSE.
- Un même périphérique monté plusieurs fois (montage de liaison, `/nix/store` en lecture seule sur NixOS, `/etc/hosts` d'un conteneur) n'apparaît qu'une fois, au point de montage le plus court. La place libre lue à chaque montage peut différer : le périphérique seul décide.
- Sous Windows (lettre de lecteur) l'étiquette du volume ne se juge pas : tout volume non vide l'est ; deux volumes de même étiquette mais de tailles différentes restent deux disques.

## Application (code)
- `crates/hearth-agent/src/domain/machine.rs::{carried_by_block_device, visible_volumes, is_real_filesystem}` (FIX:01M4EPVM3MK6PDHQDYKQGQB09R).
- `crates/hearth-agent/src/infrastructure/system/sysinfo_probe.rs::to_volumes` : la même liste alimente l'identité, les échantillons (donc « disque le plus plein ») et le rééchantillonnage de l'historique.

## Vérification
- `domain::machine::tests` : table de montage de la forge (`/dev/sda2 /`, `/dev/sda1 /boot`, `overlay …/rootfs/overlayfs/<64 hex>`), table de pseudo-systèmes, NixOS et montages de liaison, étiquettes Windows.

## Cas limites
- Un volume dont le périphérique n'est pas lisible sous Linux (nom vide) n'est pas rendu : on ne montre pas ce qu'on ne sait pas rattacher à un disque (BR-DASH-012 gardait un nom inconnu ; un nom vide sous un point de montage `/` n'est plus un disque).
- Un disque réseau monté (NFS, SMB) n'est pas rendu : ce n'est pas un disque de la machine.

## Règles liées
- BR-DASH-012.

## Historique
- 2026-10-08 : création (HRT-47, S1a, FIX-01M4EPVM3MK6PDHQDYKQGQB09R) : l'overlay de Docker de la forge apparaissait comme un disque.
