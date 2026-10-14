---
id: BR-OPS-008
domaine: OPS
titre: La reprise au démarrage compte les conteneurs du service dans le vrai moteur et ne présume jamais un état cohérent
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0 bis et 7), contexts/hearth/tickets/hrt/HRT-144.md
maj: 2026-10-14
---

# BR-OPS-008 — La reprise au démarrage compte les conteneurs du service dans le vrai moteur et ne présume jamais un état cohérent

## Règle
Pour une opération non terminée, l'agent demande au moteur (lecture seule) combien de conteneurs portent la clé du service : `c_` par l'inspection de l'identifiant, `h_` par l'étiquette `io.hearth.service-id` dans la liste, **mais l'étiquette ne vaut jamais identité** (revue de la PR 94) : `Config.Labels` contient aussi les étiquettes de l'IMAGE (`LABEL`), qu'un auteur d'image publique peut poser sans aucun accès au serveur, et l'identifiant d'un service est affiché à tous les comptes. Un conteneur n'est compté comme « service de Hearth » que si la clé de l'étiquette est celle d'un service CONNU de la base de l'agent ET si l'identifiant du conteneur est un de ceux que l'agent a consignés pour lui (`RecordedContainers`) ; tout autre conteneur qui porte l'étiquette est IGNORÉ du décompte et signalé en trace (un nombre, jamais un nom ni une étiquette). **Aujourd'hui le journal ne consigne aucun identifiant de conteneur** : pour un service connu, un conteneur étiqueté ne peut être ni reconnu ni écarté, l'état est illisible (« à vérifier »), jamais « un seul conteneur, tout va bien » ; pour une clé inconnue de la base, l'étiquette est une affirmation d'un tiers et ne compte pour rien. Le lot qui crée les conteneurs consignera leurs identifiants (deux pendant une recréation) ; `u_` n'est pas observé (illisible). Un état n'est dit COHÉRENT que si tous les gestes destructifs du plan ont leur `done` ET que le moteur montre exactement ce que la fin du plan laisse (un conteneur, ou aucun pour une suppression). Plusieurs conteneurs : ambigu (« à vérifier »). Moteur muet, liste incomplète (un conteneur non inspectable), plan à moitié exécuté ou état qui ne correspond pas : illisible (« à vérifier »). Aucune écriture dans le moteur, jamais.

## Application (code)
- `crates/hearth-agent/src/domain/ops/observe.rs::observed_from_presence`
- `crates/hearth-agent/src/infrastructure/ops_observer.rs::{EngineObserver, SpecRecordedContainers}`
- `crates/hearth-agent/src/application/ports/op_journal.rs::{RecordedContainers, RecordedService}`
- `crates/hearth-agent/src/app.rs::service_foundations_observed`

## Vérification
- `crates/hearth-agent/src/domain/ops/observe.rs::tests`
- `crates/hearth-agent/tests/service_ops_observer.rs` (dont `a_forged_label_never_counts_as_the_service_when_the_real_container_is_gone`, `a_forged_label_added_to_the_real_container_does_not_make_two_states`, `a_label_naming_a_service_the_agent_does_not_know_counts_for_nothing`, `while_the_agent_records_no_container_a_labelled_one_is_neither_trusted_nor_dismissed`)

## Cas limites
- Retirer un conteneur « créé jamais démarré » par une opération interrompue n'est pas livré : le port du moteur n'a aucune méthode qui écrit (HRT-68 ajoute `create` et `remove`). Seuls les objets de fichiers nommés au journal sont retirés.
- Les services supervisés (`u_`) ne sont pas observés ici : leurs opérations arrivent avec leur lot.

## Règles liées
- BR-EDIT-031, BR-LIFE-013, BR-EDIT-036

## Historique
- 2026-10-14 — création (HRT-144 : le socle des opérations branché sur des routes et sur le flux).
- 2026-10-14 — revue de la PR 94 : l'étiquette de conteneur ne vaut plus identité (clé connue ET identifiant consigné ; sinon ignorée ou illisible).
