/// Générateur d'identifiants techniques (ULID).
pub trait IdGen: Send + Sync {
    fn new_id(&self) -> String;
}
