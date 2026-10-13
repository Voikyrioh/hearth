---
id: BR-SVC-018
domaine: SVC
titre: Moteur non prêt : liste conservée (inaccessible) ou vidée (absent, non pris en charge, désactivé)
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-018), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-018 — Moteur non prêt : liste conservée (inaccessible) ou vidée (absent, non pris en charge, désactivé)

## Règle
Quand le moteur n'est pas prêt, la dernière liste connue reste dans le registre pour les états inaccessibles (arrêté, accès refusé, ne répond pas) : le client la grise et la date (« Liste du moteur d'il y a {âge} »). Dans les états absent, non pris en charge, version non prise en charge et désactivé, aucune liste ancienne n'est gardée : le registre se vide vraiment (conteneurs, fenêtres de démarrages, date de lecture), au moment où l'état est publié. La liste gardée (états inaccessibles) est datée par `engine.list_current_at` : l'instant où elle a cessé d'être à jour, pas la dernière relecture. Quand le moteur revient sous « prêt », l'ancienne liste (gardée ou vidée) n'est jamais présentée comme fraîche : `engine.list_degraded` reste vrai jusqu'à la première relecture complète qui suit le retour, et la liste d'un moteur absent repart vide. Le vidage est refait à chaque sonde, pas seulement à la publication : une liste lue pendant un retour retenu par l'anti-clignotement ne survit pas à une seconde disparition sans publication. « Relue » s'entend depuis la dernière fois que le moteur a été OBSERVÉ sans liste, quoi qu'il ait été publié. Réserve (compromis assumé) : sous un état inaccessible publié, les va-et-vient du moteur ne défont pas une relecture déjà faite, sinon la marque basculerait deux fois dans la fenêtre.

## Application (code)
- `crates/hearth-agent/src/domain/services/engine.rs::EngineState::{is_inaccessible, keeps_no_list}`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::commit`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::note_published`

## Vérification
- `crates/hearth-agent/src/application/services.rs::tests::an_inaccessible_engine_keeps_the_last_list_and_a_missing_one_drops_it`
- `crates/hearth-agent/src/application/services.rs::tests::a_list_from_before_the_engine_vanished_is_never_shown_as_fresh_when_it_returns`
- `crates/hearth-agent/src/domain/services/engine.rs::tests::inaccessible_states_keep_the_list_and_missing_ones_drop_it`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
- 2026-10-13 — HRT-72 : le registre est vidé pour de bon (pas masqué) et le retour du moteur n'affiche plus l'ancienne liste comme fraîche (FIX-01M4SFCQ7N5TN8VC21KBQR87PP).
- 2026-10-13 — HRT-72 (revue de la PR 78, R1) : le vidage et « liste relue » suivent l'état observé, plus l'état publié (FIX-01M4SFCQ7N5TN8VC21KBQR87PP).
