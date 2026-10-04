//! Noms des en-têtes HTTP du protocole. Les noms HTTP ne tiennent pas compte de la casse ; les
//! constantes sont en minuscules (obligatoire en HTTP/2). Forme usuelle entre parenthèses.

/// `X-Hearth-Api` : version d'interface parlée par le client (un entier).
pub const API_VERSION: &str = "x-hearth-api";

/// `X-Hearth-Api-Range` : plage `min-max` des versions acceptées, ajoutée aux réponses de l'agent.
pub const API_RANGE: &str = "x-hearth-api-range";

/// `X-Hearth-Client` : nom du poste et version du client, `poste/version`.
pub const CLIENT: &str = "x-hearth-client";

/// `Idempotency-Key` : clé d'opération (ULID) d'une requête qui modifie.
pub const IDEMPOTENCY_KEY: &str = "idempotency-key";

/// `Idempotent-Replayed` : `true` sur une réponse rejouée depuis la clé d'opération.
pub const IDEMPOTENT_REPLAYED: &str = "idempotent-replayed";
