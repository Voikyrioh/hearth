---
id: BR-SLOG-019
domaine: SLOG
titre: Le contenu des lignes ne sort jamais de la mémoire d'une lecture : ni trace, ni base, ni journal d'activité, ni file des opérations
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-019), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-14
---

# BR-SLOG-019 — Le contenu des lignes ne sort jamais de la mémoire d'une lecture : ni trace, ni base, ni journal d'activité, ni file des opérations

## Règle
L'anneau d'une lecture est le seul endroit où des lignes attendent ; il meurt avec la lecture. Aucun `Debug` d'un type qui porte une ligne n'en montre le texte (`LogLine`, `LogItem` : sa longueur seulement) ; aucune trace n'en cite ; l'adaptateur du moteur écrit son transport à la main et ne trace rien ; les cibles `bollard`, `hyper`, `tungstenite` restent sous `warn` quel que soit `RUST_LOG`. Les traces de la lecture ne portent que des nombres (ouverture, fermeture, lignes perdues). **Côté client (HRT-67, ADR-0040)** : les lignes n'existent que dans le tampon de la page, jamais sur le disque du PC ni dans `localStorage`, jamais dans un rapport d'erreur ni une trace ; la coquille Rust ne les garde pas (un événement par lot, `Debug` qui ne montre que la longueur). **Elles sont effacées dès que la lecture se ferme, par QUELQUE chemin que ce soit** : onglet quitté, détail fermé, session finie, droit retiré, et **fenêtre fermée** (la croix cache la fenêtre sans décharger la page : la coquille ferme alors les lectures et la page, avertie par `closed { window_hidden }`, remplace son tampon, lignes en attente comprises) ; il en va de même quand la coquille ferme une lecture dont la page ne donnait plus signe de vie (`closed { idle }`) ou dont le serveur est déconnecté, changé ou oublié (`closed { server_changed }`). Au retour, l'écran le dit et propose « Reprendre » : aucune reprise silencieuse. La coupe d'une ligne longue tombe sur une frontière de caractère.

## Application (code)
- `apps/desktop/src-tauri/src/window.rs::{on_window_event, close_log_reads}` et `apps/desktop/src-tauri/src/services/logs.rs::{LogSessions::close_window, LogSessions::forget_window, LogSessions::close_server, LogSessions::reap}`
- `apps/desktop/src/stores/serviceLogs.ts` (fin `closed` : le tampon est remplacé) et `apps/desktop/src/logs/text.ts::cutOnCharacter`
- `crates/hearth-agent/src/domain/services/logs.rs::LogLine (Debug)`
- `crates/hearth-proto/src/api/service_logs.rs::LogItem (Debug)`
- `crates/hearth-agent/src/infrastructure/logging.rs::TransportGuard`
- `crates/hearth-agent/src/entrypoint/ws/service_logs.rs::serve`

## Vérification
- `apps/desktop/src-tauri/tests/service_logs_runtime.rs::hiding_the_window_closes_its_reads_tells_the_page_and_gives_the_places_back`
- `apps/desktop/src/stores/serviceLogs.test.ts` : « la coquille ferme la lecture : les lignes sont effacées et l'écran propose de reprendre »
- `apps/desktop/e2e/hrt67-shell.spec.ts` : « fenêtre cachée »
- Ne se prouve que dans la vraie fenêtre : l'événement `CloseRequested` de la croix (voir ADR-0040, limites).
- `crates/hearth-agent/tests/service_logs_ws.rs::no_line_reaches_a_trace_the_database_the_journal_or_the_operations_queue (sentinelle, chemin d'initialisation de la production, niveau le plus bavard ; mutation vérifiée à la main)`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::the_text_of_a_line_never_appears_in_debug`
- `crates/hearth-proto/src/api/service_logs.rs::tests::the_text_of_a_line_never_appears_in_debug`

## Règles liées
- BR-SLOG-021, BR-SLOG-022

## Historique
- 2026-10-14 : côté client, effacement à la fermeture par la coquille (fenêtre cachée, page muette, serveur changé), revue de la PR 95.
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
