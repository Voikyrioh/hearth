//! Installation gérée par le système (NixOS par exemple) : l'agent n'écrit aucune unité, ne
//! lance rien. Le système déclare et démarre le service lui-même.

use crate::application::ports::{ServiceError, ServiceKind, ServiceManager, ServiceSpec};

pub struct Unmanaged;

impl ServiceManager for Unmanaged {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Unmanaged
    }

    fn is_installed(&self) -> Result<bool, ServiceError> {
        Ok(false)
    }

    fn is_active(&self) -> Result<bool, ServiceError> {
        Ok(false)
    }

    fn install(&self, _spec: &ServiceSpec) -> Result<(), ServiceError> {
        Ok(())
    }

    fn unit_text(&self) -> Result<Option<String>, ServiceError> {
        Ok(None)
    }

    fn restore_unit(&self, _text: &str) -> Result<(), ServiceError> {
        Ok(())
    }

    fn restart(&self) -> Result<(), ServiceError> {
        Ok(())
    }

    fn stop(&self) -> Result<(), ServiceError> {
        Ok(())
    }

    fn disable(&self) -> Result<(), ServiceError> {
        Ok(())
    }

    fn remove(&self) -> Result<(), ServiceError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn nothing_is_written_nothing_runs_and_nothing_is_reported_installed() {
        let manager = Unmanaged;
        assert_eq!(manager.kind(), ServiceKind::Unmanaged);
        let spec = ServiceSpec {
            binary: PathBuf::from("/x"),
            config: PathBuf::from("/y"),
            data_dir: PathBuf::from("/z"),
        };
        assert!(manager.install(&spec).is_ok());
        assert!(manager.restart().is_ok());
        assert!(manager.stop().is_ok());
        assert!(manager.disable().is_ok());
        assert!(manager.remove().is_ok());
        assert!(!manager.is_installed().unwrap());
        assert!(!manager.is_active().unwrap());
    }
}
