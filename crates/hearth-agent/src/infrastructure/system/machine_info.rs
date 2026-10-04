use mac_address::MacAddressIterator;

use crate::application::ports::MachineInfo;

/// Lit le nom de machine et les adresses MAC auprès du système d'exploitation.
#[derive(Debug, Default)]
pub struct SystemMachineInfo;

impl MachineInfo for SystemMachineInfo {
    fn machine_name(&self) -> String {
        gethostname::gethostname().to_string_lossy().into_owned()
    }

    fn mac_addresses(&self) -> Vec<String> {
        let Ok(interfaces) = MacAddressIterator::new() else {
            tracing::warn!("lecture des adresses MAC impossible");
            return Vec::new();
        };
        let mut macs: Vec<String> = interfaces
            .map(|mac| mac.bytes())
            // Les interfaces virtuelles (boucle locale…) remontent parfois une adresse nulle.
            .filter(|bytes| bytes.iter().any(|b| *b != 0))
            .map(format_mac)
            .collect();
        macs.sort();
        macs.dedup();
        macs
    }
}

fn format_mac(bytes: [u8; 6]) -> String {
    let parts: Vec<String> = bytes.iter().map(|b| format!("{b:02X}")).collect();
    parts.join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_a_mac_in_uppercase_with_colons() {
        assert_eq!(
            format_mac([0xaa, 0x0b, 0xcc, 0x0d, 0xee, 0xff]),
            "AA:0B:CC:0D:EE:FF"
        );
    }

    #[test]
    fn machine_name_is_not_empty() {
        assert!(!SystemMachineInfo.machine_name().is_empty());
    }
}
