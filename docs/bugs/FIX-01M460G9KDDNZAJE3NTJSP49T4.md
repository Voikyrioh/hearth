---
id: FIX-01M460G9KDDNZAJE3NTJSP49T4
titre: L'unité systemd retirait CAP_MKNOD : nvidia-smi ne voyait pas la carte d'un serveur sans écran
date_découverte: 2026-10-05
date_correction: 2026-10-05
---

# FIX-01M460G9KDDNZAJE3NTJSP49T4 : L'unité systemd retirait CAP_MKNOD : nvidia-smi ne voyait pas la carte d'un serveur sans écran

## Symptôme
Sur un serveur sans écran, le premier `nvidia-smi` doit créer `/dev/nvidia*` (`mknod`) ; sans `CAP_MKNOD` il échoue et la carte graphique n'est jamais vue.

## Cause root
`CapabilityBoundingSet` de l'unité générée n'incluait pas `CAP_MKNOD` (ADR-0012 la retirait).

## Impacté
Installation et désinstallation depuis HRT-15.

## Workaround
Aucun.

## Correction
`CAP_MKNOD` est gardée (`infrastructure/service/systemd.rs::render_unit`), ADR-0012 mise à jour, test `the_capability_set_keeps_mknod_for_the_first_nvidia_smi_on_a_headless_server`.

## Références
- Ticket : HRT-17 (suivi de la review de HRT-15, PR #11)
- BR : BR-INSTALL-005 ; code : `crates/hearth-agent/src/infrastructure/service/systemd.rs` (marqueur `FIX:01M460G9KDDNZAJE3NTJSP49T4`)
