//! Le rapprochement entre ce qui est VOULU (la description) et ce qui est OBSERVÉ (le conteneur) :
//! fonction pure (HRT-83, ADR-0044 point 6, BR-EDIT-024).
//!
//! Trois issues : rien à dire, « Modifié hors de Hearth », « Conteneur absent ». Un écart est **affiché,
//! jamais corrigé seul** : ce module ne fait que le constater, il ne demande rien au moteur. Un service
//! dont le conteneur a disparu reste dans la liste, avec le seul geste « Recréer le service ».

use std::collections::BTreeSet;

use hearth_proto::api::service_spec::DriftView;

use super::spec::{SpecBody, SpecMount, SpecMountKind, SpecPort, SpecRestart};

/// Ce que le moteur dit du conteneur, ramené aux seuls réglages que la description pose. Fabriqué par
/// l'adaptateur du moteur ; le domaine n'y voit ni identifiant, ni état, ni mesure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedContainer {
    /// Révision portée par l'étiquette `io.hearth.spec-rev`.
    pub label_rev: Option<u32>,
    /// L'étiquette `io.hearth.spec-mac` est celle de cette révision (calculée par l'adaptateur sous la
    /// clé des services : le domaine ne sait que le résultat). Faux si la clé est illisible : on ne
    /// prétend alors pas « conforme ».
    pub label_mac_valid: bool,
    pub image_repository: String,
    pub image_tag: String,
    pub ports: Vec<SpecPort>,
    pub mounts: Vec<SpecMount>,
    pub restart: SpecRestart,
    pub cpu_millis: Option<u32>,
    pub memory_bytes: Option<u64>,
}

/// Ce qui diffère. Liste fermée, pour les tests et le journal ; le fil n'en garde que [`DriftView`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DriftField {
    /// Le conteneur ne porte pas l'étiquette de cette révision, ou son code est faux.
    Labels,
    Image,
    Ports,
    Mounts,
    Restart,
    Cpu,
    Memory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drift {
    None,
    ModifiedOutside(Vec<DriftField>),
    ContainerMissing,
}

impl Drift {
    /// La forme du fil.
    pub fn view(&self) -> DriftView {
        match self {
            Self::None => DriftView::None,
            Self::ModifiedOutside(_) => DriftView::ModifiedOutside,
            Self::ContainerMissing => DriftView::ContainerMissing,
        }
    }
}

/// Compare la révision COURANTE de la description à l'observation (`None` = le conteneur n'existe pas).
pub fn compare(rev: u32, wanted: &SpecBody, observed: Option<&ObservedContainer>) -> Drift {
    let Some(observed) = observed else {
        return Drift::ContainerMissing;
    };
    let mut differs = BTreeSet::new();
    if observed.label_rev != Some(rev) || !observed.label_mac_valid {
        differs.insert(DriftField::Labels);
    }
    if observed.image_repository != wanted.image.repository
        || observed.image_tag != wanted.image.tag
    {
        differs.insert(DriftField::Image);
    }
    if as_set(&observed.ports) != as_set(&wanted.ports) {
        differs.insert(DriftField::Ports);
    }
    if mounts_key(&observed.mounts) != mounts_key(&wanted.mounts) {
        differs.insert(DriftField::Mounts);
    }
    if observed.restart != wanted.restart {
        differs.insert(DriftField::Restart);
    }
    if observed.cpu_millis != wanted.cpu_millis {
        differs.insert(DriftField::Cpu);
    }
    if observed.memory_bytes != wanted.memory_bytes {
        differs.insert(DriftField::Memory);
    }
    if differs.is_empty() {
        Drift::None
    } else {
        Drift::ModifiedOutside(differs.into_iter().collect())
    }
}

fn as_set<T: Ord + Clone>(items: &[T]) -> BTreeSet<T> {
    items.iter().cloned().collect()
}

/// Les montages comparés par ce que le moteur peut redire : cible, lecture seule, et source. Un
/// emplacement géré est vu par le moteur comme un chemin sous la racine de Hearth : l'adaptateur le
/// ramène à son nom avant de remplir l'observation.
fn mounts_key(mounts: &[SpecMount]) -> BTreeSet<(SpecMountKind, String, String, bool)> {
    mounts
        .iter()
        .map(|mount| {
            (
                mount.kind,
                mount.source.clone(),
                mount.target.clone(),
                mount.read_only,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::spec::{SpecImage, SpecProtocol, SpecSettings};
    use super::*;

    fn wanted() -> SpecBody {
        SpecBody {
            name: "Terraria".into(),
            image: SpecImage {
                repository: "ghcr.io/acme/terraria".into(),
                tag: "1.4.4".into(),
                digest: None,
            },
            ports: vec![
                SpecPort {
                    host_port: 7777,
                    container_port: 7777,
                    protocol: SpecProtocol::Tcp,
                },
                SpecPort {
                    host_port: 7878,
                    container_port: 7878,
                    protocol: SpecProtocol::Udp,
                },
            ],
            mounts: vec![SpecMount {
                kind: SpecMountKind::Managed,
                source: "world".into(),
                target: "/data".into(),
                read_only: false,
            }],
            restart: SpecRestart::UnlessStopped,
            stop_grace_s: None,
            stdin_open: false,
            cpu_millis: Some(500),
            memory_bytes: Some(1 << 30),
            settings: SpecSettings::default(),
            fiche: None,
        }
    }

    fn observed_like(spec: &SpecBody, rev: u32) -> ObservedContainer {
        ObservedContainer {
            label_rev: Some(rev),
            label_mac_valid: true,
            image_repository: spec.image.repository.clone(),
            image_tag: spec.image.tag.clone(),
            ports: spec.ports.iter().rev().copied().collect(),
            mounts: spec.mounts.clone(),
            restart: spec.restart,
            cpu_millis: spec.cpu_millis,
            memory_bytes: spec.memory_bytes,
        }
    }

    #[test]
    fn a_container_that_matches_is_conform_whatever_the_order_of_its_ports() {
        let spec = wanted();
        assert_eq!(
            compare(3, &spec, Some(&observed_like(&spec, 3))),
            Drift::None
        );
    }

    #[test]
    fn no_container_is_reported_as_missing_and_nothing_else() {
        let drift = compare(1, &wanted(), None);
        assert_eq!(drift, Drift::ContainerMissing);
        assert_eq!(drift.view(), DriftView::ContainerMissing);
    }

    #[test]
    fn each_difference_is_named_and_reads_as_modified_outside() {
        let spec = wanted();
        type Change = fn(&mut ObservedContainer);
        let cases: [(Change, DriftField); 8] = [
            (|o| o.label_rev = Some(2), DriftField::Labels),
            (|o| o.label_rev = None, DriftField::Labels),
            (|o| o.label_mac_valid = false, DriftField::Labels),
            (|o| o.image_tag = "latest".into(), DriftField::Image),
            (
                |o| {
                    o.ports.pop();
                },
                DriftField::Ports,
            ),
            (|o| o.mounts[0].read_only = true, DriftField::Mounts),
            (|o| o.restart = SpecRestart::Always, DriftField::Restart),
            (|o| o.memory_bytes = None, DriftField::Memory),
        ];
        for (change, field) in cases {
            let mut observed = observed_like(&spec, 3);
            change(&mut observed);
            let drift = compare(3, &spec, Some(&observed));
            assert_eq!(drift, Drift::ModifiedOutside(vec![field]), "{field:?}");
            assert_eq!(drift.view(), DriftView::ModifiedOutside);
        }
        let mut cpu = observed_like(&spec, 3);
        cpu.cpu_millis = Some(1000);
        assert_eq!(
            compare(3, &spec, Some(&cpu)),
            Drift::ModifiedOutside(vec![DriftField::Cpu])
        );
    }

    #[test]
    fn several_differences_are_all_reported_in_a_stable_order() {
        let spec = wanted();
        let mut observed = observed_like(&spec, 3);
        observed.memory_bytes = Some(1);
        observed.image_tag = "x".into();
        observed.label_rev = Some(1);
        assert_eq!(
            compare(3, &spec, Some(&observed)),
            Drift::ModifiedOutside(vec![
                DriftField::Labels,
                DriftField::Image,
                DriftField::Memory
            ])
        );
    }

    #[test]
    fn a_container_made_by_an_older_revision_is_not_silently_conform() {
        // Après une coupure entre l'écriture de la révision et la création du conteneur : dit, pas corrigé.
        let spec = wanted();
        let observed = observed_like(&spec, 2);
        assert_eq!(
            compare(3, &spec, Some(&observed)),
            Drift::ModifiedOutside(vec![DriftField::Labels])
        );
    }
}
