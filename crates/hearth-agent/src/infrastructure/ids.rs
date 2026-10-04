//! Identifiants techniques : ULID (triables par date de création).

use crate::application::ports::IdGen;

pub struct UlidGen;

impl IdGen for UlidGen {
    fn new_id(&self) -> String {
        ulid::Ulid::generate().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_26_characters_and_unique() {
        let ids = UlidGen;
        let first = ids.new_id();
        assert_eq!(first.len(), 26);
        assert_ne!(first, ids.new_id());
    }
}
