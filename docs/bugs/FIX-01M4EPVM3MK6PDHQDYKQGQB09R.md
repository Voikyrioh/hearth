---
id: FIX-01M4EPVM3MK6PDHQDYKQGQB09R
titre: Un volume overlay de Docker rendu comme un disque (S1a)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4EPVM3MK6PDHQDYKQGQB09R : Un volume overlay de Docker rendu comme un disque (S1a)

## Symptôme
Une ligne `overlay` montée sur `/var/lib/docker/rootfs/overlayfs/<64 hex>`, avec les mêmes octets que `/`, apparaissait dans la carte Disques et dans la ligne « Disques » de la carte Machine.

## Reproduction
`domain::machine::tests::the_docker_overlay_of_the_forge_is_not_a_disk` et deux autres (rouges avant), table de montage de la capture du smoke.

## Cause root
`overlay` était déclaré système de fichiers « réel » (test compris) et aucune règle ne demandait un périphérique bloc.

## Impacté
Carte Disques, carte Machine, « disque le plus plein », historique d'une heure (même liste, même fonction). Source : premier smoke de Voiky, 2026-10-08 (ticket HRT-47).

## Workaround
Aucun.

## Correction
Règle écrite BR-DASH-016 et codée dans `domain/machine.rs` : seuls les volumes portés par un `/dev/…` (ou un jeu ZFS) sont rendus ; `overlay` rejoint les systèmes de service ; un même périphérique monté plusieurs fois (dont `/nix/store`) n'apparaît qu'une fois quelle que soit la place libre lue. S1b (chemin long dans la carte) reste à faire côté interface.

## Règles
- BR-DASH-016 créée, BR-DASH-012 renvoie à elle.
