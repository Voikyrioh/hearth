---
id: BR-COMPAT-001
domaine: COMPAT
titre: Chaque côté tolère ce qu'il ne connaît pas dans ce qu'il lit de l'autre ; une valeur inconnue n'est jamais prise pour une valeur connue
statut: active
invariant: true
source: revue de la première version publique (2026-10-14), FIX-01M4VR6Y4Q4AVQVT9FCME7Z2SB, BR-GRANT-016
maj: 2026-10-14
---

# BR-COMPAT-001 : Chaque côté tolère ce qu'il ne connaît pas dans ce qu'il lit de l'autre

## Règle
L'agent et le client évoluent séparément (un utilisateur n'a pas toujours mis son client à jour quand l'agent l'est, et inversement). Donc :

1. **Énumération lue de l'autre côté** (réponse, message du flux, événement, détail d'erreur) : elle a UNE variante de repli, `#[serde(other)] Unknown` (le système d'init a `Other`, qui dit déjà « un autre que ceux-là »). Une valeur inconnue ne fait JAMAIS rejeter le message entier. C'est le seul modèle du dépôt ; pas de type enveloppe, pas de `Option` pour dire « inconnu ».
2. **Message du flux** (`ServerMessage`) : un `type` inconnu se lit `ServerMessage::Unknown`, la liaison l'ignore et le flux continue. Les messages `security` et `services` sont lus AVANT (`decode_frame`), car `Unknown` accepte tout `type`.
3. **Structure** : un champ en trop est ignoré (jamais `deny_unknown_fields`, jamais un `Deserialize` écrit à la main, sur ce qui est lu du réseau). Un champ ajouté au protocole est facultatif (`Option` ou `#[serde(default)]`). On ne change jamais le type d'un champ existant (un nombre ne s'élargit pas : on ajoute un champ).
4. **Une valeur inconnue n'est JAMAIS prise pour une valeur connue ni pour un succès**. Côté client : une étape inconnue = « étape que cette version ne connaît pas » (l'écran dit « en cours » sans en marquer une), une issue inconnue = ni réussite ni échec affirmés, un état d'opération inconnu = « résultat inconnu » (jamais rejoué), un motif de fin de session inconnu = ni expirée ni révoquée affirmée, un état du mode attaque inconnu ne change pas ce que le client savait. Le repli reste une valeur à part jusqu'à l'interface (`"unknown"` dans `bindings.ts`).
5. **Ce que l'AGENT lit du client est une commande, et reste STRICT** : une valeur inconnue dans une commande est un refus propre et typé (`422 VALIDATION_ERROR` avec `details.field`, avant toute confirmation de l'acte), jamais une action par défaut. Un rôle inconnu n'est ni lecture seule ni administrateur ; un réglage inconnu n'est jamais écrit ; un usage de défi inconnu ne reçoit pas de défi. Les messages du client au flux (`ClientMessage`, `SignedAuth`, `Topic`, `LogsClientMessage`) restent stricts : un sujet inconnu refuse le message `subscribe` (le client n'envoie un sujet qu'à un agent qui annonce la capacité).
6. **`API_VERSION` ne bouge pas** pour une valeur ajoutée à une énumération tolérante, ni pour un champ facultatif : c'est tout l'intérêt de cette règle. Elle bouge seulement quand un côté ne peut plus fonctionner avec l'autre.

## Application (code)
- `crates/hearth-proto/src/**` : variantes `Unknown`/`Other` ; `ErrorCode::Unknown` (statut 500, jamais émis par l'agent) ; `ServerMessage::Unknown`.
- `crates/hearth-link/src/adapters/http_transport.rs::decode_frame` (ordre des lectures), `manager/task.rs::{on_message, on_update}`, `manager/attempt.rs::upgrade_target`, `domain/pending_ops.rs::decide`.
- `apps/desktop/src-tauri/src/**/dto.rs` et `link_dto.rs` : chaque DTO miroir porte `Unknown` ; `presence.rs::SecurityWatch::observe`.
- Agent, commandes strictes : `entrypoint/http/{reauth.rs::Parsed::read, wire.rs::role_from_wire, security.rs::set_reauth, sessions.rs::challenge}`, `application/trust.rs::usage_of`.
- `hearth-proto::admin_act::covered_by_elevation` : seul un rôle lecture seule est couvert par l'élévation (un rôle inconnu se traite comme administrateur).

## Vérification
- `crates/hearth-proto/tests/tolerance.rs` : **le garde**. Il lit le code de `hearth-proto`, exige que toute `pub enum` qui dérive `Deserialize` soit classée (tolérante, stricte ou enveloppe) dans une liste tenue à la main ET qu'aucune entrée ne soit périmée, joue chaque énumération tolérante avec `"__valeur_du_futur__"` (ou un `type` inventé) et exige la variante `Unknown` ; scanne `hearth-proto`, `hearth-link` et le client Tauri pour `deny_unknown_fields` et `Deserialize` écrit à la main ; rejoue des structures avec un champ en trop.
- `crates/hearth-link/tests/unknown_agent_values.rs` (agent « du futur » de `tests/support/future_agent.rs`, vrai `HttpTransport`, vrai TLS 1.3) : étape, issue, raison, type de message, code d'erreur et motif de fin de session inconnus ; le flux continue.
- `http_transport::tests::frames_are_routed_by_their_type_and_an_unknown_type_is_ignored`.
- Refus de l'agent : `entrypoint::http::{reauth,wire}::tests`, `application::trust::tests`, `tests/device_proof.rs::a_challenge_for_a_purpose_this_version_does_not_know_is_refused`.

## Cas limites
- Un repli ne dit pas tout : une raison de mot de passe faible inconnue (`details.rules`) est omise de la liste, le refus reste « mot de passe faible ». `LogSource::Unknown` se traite comme la sortie normale (décision de HRT-66).
- Un nombre hors des bornes de son type (`percent` hors `u8`) fait toujours rejeter le message : d'où l'interdiction d'élargir un type.
