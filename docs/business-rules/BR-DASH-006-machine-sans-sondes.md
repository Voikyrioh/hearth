---
id: BR-DASH-006
domaine: DASH
titre: Une machine sans sonde de température affiche un texte explicatif
statut: partielle
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-006), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-05
---

# BR-DASH-006 — Une machine sans sonde de température affiche un texte explicatif

## Règle
Côté agent : sans sonde exposée par le système, `capabilities.temps = false` et `temps` vide dans chaque échantillon. Le texte explicatif est l'affaire du client (HRT-11). La température d'une carte graphique est indépendante (`gpus[].temp_c`).

## Application (code)
- `crates/hearth-agent/src/domain/machine.rs::MachineIdentity::capabilities`.

## Vérification
- `domain::machine::tests`
- `infrastructure::system::sysinfo_probe::tests`

## Cas limites
- Une sonde dont la valeur est illisible ou aberrante (hors de -50 à 150 °C) est ignorée, pas rendue à zéro.

## Règles liées
- BR-DASH-008

## Interface
- `components/organisms/TemperaturesCard.vue` : `capabilities.temps` faux : « Sondes non disponibles sur cette machine. Ce matériel n'expose pas sa température au système. ». Tests : `pages/Dashboard.test.ts`, `e2e/dashboard.spec.ts`.

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
- 2026-10-05 — interface du tableau de bord (HRT-11, session 2026-10-04-hearth-creation).
