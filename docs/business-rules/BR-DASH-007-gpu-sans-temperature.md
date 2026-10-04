---
id: BR-DASH-007
domaine: DASH
titre: Une carte graphique sans mesure de température affiche « Non disponible »
statut: partielle
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-007), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-04
---

# BR-DASH-007 — Une carte graphique sans mesure de température affiche « Non disponible »

## Règle
Côté agent : `gpus[].temp_c` est absent (`null`) quand la carte n'expose pas sa température (`[N/A]` pour `nvidia-smi`, fichier absent pour le noyau) ; charge et mémoire vidéo restent présentes si elles sont lisibles. Chaque champ est indépendant.

## Application (code)
- `crates/hearth-agent/src/infrastructure/system/gpu/nvidia_smi.rs::parse_line`.
- `crates/hearth-agent/src/infrastructure/system/gpu/sysfs.rs::read_card`.
- `crates/hearth-proto/src/api/metrics.rs::GpuSample` (champs optionnels).

## Vérification
- `gpu::nvidia_smi::tests` (échantillons figés dont `[N/A]`)
- `gpu::sysfs::tests`

## Cas limites
- Un champ illisible ne rend pas l'échantillon invalide ; une ligne entièrement illisible est ignorée.

## Règles liées
- BR-DASH-005, BR-DASH-008

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
