---
id: BR-SVC-024
domaine: SVC
titre: Lire la liste des services n'est pas journalisé
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-024), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-024 — Lire la liste des services n'est pas journalisé

## Règle
Lire la liste, le détail et le récapitulatif des ports sont des consultations, non journalisées. Dans cette tranche : `GET /services`, `POST /engine/recheck` et le sujet `services`, sans action de journal. La révélation des valeurs (journalisée) arrive avec la tranche 2.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS (`audit: None`)`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::reading_the_engine_and_the_services_writes_nothing_to_the_journal`

## Règles liées
- BR-ENGINE-020

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
