---
id: BR-DASH-003
domaine: DASH
titre: Un état d'alerte comporte trois niveaux : normal, attention, critique
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-003), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-04
---

# BR-DASH-003 — Un état d'alerte comporte trois niveaux : normal, attention, critique

## Règle
Mémoire, mémoire vidéo, disque et processeur : attention à partir de 85 %, critique à partir de 95 %. Températures (processeur, carte graphique, sondes) : attention à partir de 80 °C, critique à partir de 90 °C. Les bornes sont incluses. Fonctions pures partagées avec le client, qui les applique sur les séries reçues. Une mesure illisible ou un total nul est normal : on n'alerte pas sur ce qu'on n'a pas.

## Application (code)
- `crates/hearth-proto/src/thresholds.rs::{percent_level, usage_level, temperature_level}`, `Level`.

## Vérification
- `hearth_proto::thresholds::tests` (bornes exactes, grandes quantités, NaN, total nul)

## Cas limites
- `usage_level` calcule en entiers : aucune erreur d'arrondi aux bornes. Une valeur supérieure à 100 % est critique.

## Règles liées
- BR-DASH-004

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
