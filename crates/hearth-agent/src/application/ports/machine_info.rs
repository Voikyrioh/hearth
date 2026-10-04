/// Informations d'identification de la machine hôte.
pub trait MachineInfo: Send + Sync {
    fn machine_name(&self) -> String;

    /// Adresses MAC des interfaces, au format `AA:BB:CC:DD:EE:FF`, sans doublon.
    fn mac_addresses(&self) -> Vec<String>;
}
