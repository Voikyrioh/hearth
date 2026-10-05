//! Identité de la machine et règles de ce qui compte comme disque ou interface réseau
//! (BR-DASH-005, BR-DASH-006, BR-DASH-012). Aucune lecture du système ici : les sondes
//! observent, ces fonctions décident.

use std::collections::HashSet;

/// Ce que la machine sait mesurer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub gpu: bool,
    pub temps: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsIdentity {
    pub name: String,
    pub version: Option<String>,
    pub kernel: Option<String>,
    pub arch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuIdentity {
    pub model: String,
    pub physical_cores: Option<u32>,
    pub logical_cores: u32,
    pub frequency_mhz: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskIdentity {
    pub name: String,
    pub mount: String,
    pub fs: Option<String>,
    pub total_bytes: u64,
    pub removable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuIdentity {
    pub name: String,
    pub memory_total_bytes: Option<u64>,
}

/// Ce que l'agent sait de la machine. Les cartes graphiques sont ajoutées par le cas d'usage
/// (la sonde système ne les connaît pas).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineIdentity {
    pub name: String,
    pub os: OsIdentity,
    pub cpu: CpuIdentity,
    pub memory_total_bytes: u64,
    pub disks: Vec<DiskIdentity>,
    pub gpus: Vec<GpuIdentity>,
    /// Le système expose au moins une sonde de température.
    pub has_temperature_sensors: bool,
}

impl MachineIdentity {
    /// Une machine sans carte graphique ou sans sonde le dit par ses capacités : pas d'erreur,
    /// pas de valeur inventée (BR-DASH-005, BR-DASH-006).
    pub fn capabilities(&self) -> Capabilities {
        Capabilities {
            gpu: !self.gpus.is_empty(),
            temps: self.has_temperature_sensors,
        }
    }
}

/// Un volume monté, tel que le système le liste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub name: String,
    pub mount: String,
    pub fs: Option<String>,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub removable: bool,
}

impl Volume {
    pub fn used_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.available_bytes)
    }
}

/// Systèmes de fichiers de service : pas des disques (BR-DASH-012).
const SERVICE_FILESYSTEMS: &[&str] = &[
    "autofs",
    "binfmt_misc",
    "bpf",
    "cgroup",
    "cgroup2",
    "configfs",
    "debugfs",
    "devpts",
    "devtmpfs",
    "efivarfs",
    "fusectl",
    "hugetlbfs",
    "mqueue",
    "nsfs",
    "proc",
    "pstore",
    "ramfs",
    "rpc_pipefs",
    "securityfs",
    "squashfs",
    "sysfs",
    "tmpfs",
    "tracefs",
];

/// Ce système de fichiers est-il celui d'un vrai disque ? Un nom inconnu ou vide l'est (on ne
/// cache pas un disque que l'on ne sait pas nommer).
pub fn is_real_filesystem(fs: &str) -> bool {
    let fs = fs.trim().to_ascii_lowercase();
    !SERVICE_FILESYSTEMS.contains(&fs.as_str())
}

/// Les disques à montrer parmi les volumes listés : systèmes de fichiers de service, volumes de
/// taille nulle et doublons (un même volume monté à plusieurs endroits, ou lié par `bind`) sont
/// retirés ; on garde le point de montage le plus court. Résultat trié par point de montage.
pub fn visible_volumes(mut volumes: Vec<Volume>) -> Vec<Volume> {
    volumes.retain(|volume| {
        volume.total_bytes > 0 && volume.fs.as_deref().is_none_or(is_real_filesystem)
    });
    volumes.sort_by(|a, b| {
        a.mount
            .len()
            .cmp(&b.mount.len())
            .then_with(|| a.mount.cmp(&b.mount))
    });
    let mut seen = HashSet::new();
    volumes.retain(|volume| {
        let device = if volume.name.is_empty() {
            &volume.mount
        } else {
            &volume.name
        };
        seen.insert((device.clone(), volume.total_bytes, volume.available_bytes))
    });
    volumes.sort_by(|a, b| a.mount.cmp(&b.mount));
    volumes
}

/// Ce que l'observation dit d'une interface réseau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceKind {
    /// L'interface a un périphérique matériel derrière elle (carte réseau, adaptateur Wi-Fi).
    pub physical: bool,
    /// C'est le bouclage local.
    pub loopback: bool,
}

/// Quelles interfaces comptent dans le débit de la machine (BR-DASH-015) : les interfaces
/// physiques, dont le trafic est le vrai trafic de la machine. Conteneurs, ponts, tunnels,
/// VPN et agrégats font repasser leur trafic par une interface physique : les compter
/// doublerait le débit. Si l'observation ne trouve aucune interface physique (système qui ne
/// l'expose pas, mode dev), on se rabat sur toutes les interfaces sauf le bouclage.
///
/// `interfaces` : les interfaces observées ; rend, pour chacune, si elle compte.
pub fn throughput_interfaces(interfaces: &[InterfaceKind]) -> Vec<bool> {
    let any_physical = interfaces
        .iter()
        .any(|interface| interface.physical && !interface.loopback);
    interfaces
        .iter()
        .map(|interface| !interface.loopback && (interface.physical || !any_physical))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(name: &str, mount: &str, fs: &str, total: u64, available: u64) -> Volume {
        Volume {
            name: name.into(),
            mount: mount.into(),
            fs: Some(fs.into()),
            total_bytes: total,
            available_bytes: available,
            removable: false,
        }
    }

    fn identity(gpus: usize, sensors: bool) -> MachineIdentity {
        MachineIdentity {
            name: "forge".into(),
            os: OsIdentity {
                name: "NixOS".into(),
                version: None,
                kernel: None,
                arch: "x86_64".into(),
            },
            cpu: CpuIdentity {
                model: "cpu".into(),
                physical_cores: None,
                logical_cores: 4,
                frequency_mhz: None,
            },
            memory_total_bytes: 1,
            disks: vec![],
            gpus: (0..gpus)
                .map(|i| GpuIdentity {
                    name: format!("gpu{i}"),
                    memory_total_bytes: None,
                })
                .collect(),
            has_temperature_sensors: sensors,
        }
    }

    #[test]
    fn capabilities_follow_what_the_machine_exposes() {
        let none = identity(0, false).capabilities();
        assert_eq!((none.gpu, none.temps), (false, false));
        let sensors_only = identity(0, true).capabilities();
        assert_eq!((sensors_only.gpu, sensors_only.temps), (false, true));
        let gpu_only = identity(2, false).capabilities();
        assert_eq!((gpu_only.gpu, gpu_only.temps), (true, false));
    }

    #[test]
    fn service_filesystems_are_not_disks() {
        for fs in [
            "tmpfs",
            "proc",
            "SYSFS",
            "cgroup2",
            "squashfs",
            " devtmpfs ",
        ] {
            assert!(!is_real_filesystem(fs), "{fs}");
        }
        for fs in [
            "ext4", "btrfs", "xfs", "NTFS", "exfat", "overlay", "zfs", "",
        ] {
            assert!(is_real_filesystem(fs), "{fs}");
        }
    }

    #[test]
    fn visible_volumes_drop_service_filesystems_empty_volumes_and_duplicates() {
        let volumes = vec![
            volume("/dev/sda2", "/", "ext4", 100, 40),
            volume("/dev/sda2", "/nix/store", "ext4", 100, 40),
            volume("tmpfs", "/run", "tmpfs", 10, 10),
            volume("/dev/sdb1", "/mnt/usb", "vfat", 0, 0),
            volume("/dev/sdc1", "/mnt/data", "ext4", 500, 100),
        ];
        let kept = visible_volumes(volumes);
        let mounts: Vec<_> = kept.iter().map(|v| v.mount.as_str()).collect();
        assert_eq!(mounts, ["/", "/mnt/data"]);
    }

    #[test]
    fn two_distinct_volumes_with_the_same_label_are_both_kept() {
        let volumes = vec![
            volume("Data", "D:\\", "NTFS", 100, 40),
            volume("Data", "E:\\", "NTFS", 200, 40),
        ];
        assert_eq!(visible_volumes(volumes).len(), 2);
    }

    #[test]
    fn a_volume_without_a_name_is_identified_by_its_mount() {
        let volumes = vec![
            volume("", "C:\\", "NTFS", 100, 40),
            volume("", "D:\\", "NTFS", 100, 40),
        ];
        assert_eq!(visible_volumes(volumes).len(), 2);
    }

    #[test]
    fn used_bytes_never_underflows() {
        assert_eq!(volume("a", "/", "ext4", 100, 30).used_bytes(), 70);
        assert_eq!(volume("a", "/", "ext4", 100, 130).used_bytes(), 0);
    }

    fn kind(physical: bool, loopback: bool) -> InterfaceKind {
        InterfaceKind { physical, loopback }
    }

    #[test]
    fn only_physical_interfaces_count_when_there_are_some() {
        // lo, eth0 (physique), docker0, tailscale0, bond0
        let observed = [
            kind(false, true),
            kind(true, false),
            kind(false, false),
            kind(false, false),
            kind(false, false),
        ];
        assert_eq!(
            throughput_interfaces(&observed),
            [false, true, false, false, false]
        );
    }

    #[test]
    fn without_any_physical_interface_everything_but_the_loopback_counts() {
        let observed = [kind(false, true), kind(false, false), kind(false, false)];
        assert_eq!(throughput_interfaces(&observed), [false, true, true]);
    }

    #[test]
    fn a_loopback_never_counts_even_if_it_claims_a_device() {
        let observed = [kind(true, true), kind(true, false)];
        assert_eq!(throughput_interfaces(&observed), [false, true]);
        assert_eq!(throughput_interfaces(&[kind(true, true)]), [false]);
        assert!(throughput_interfaces(&[]).is_empty());
    }
}
