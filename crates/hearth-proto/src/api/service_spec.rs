//! La description durable d'un service tel que le client la voit (HRT-83, ADR-0044) : de quoi afficher
//! l'écart avec le moteur, **jamais une valeur**. Ni valeur de variable, ni commande, ni réponse secrète
//! d'une fiche : les secrets d'une description restent scellés sur le serveur (BR-EDIT-034). Les noms de
//! variables sont scellés eux aussi dans la base ; une route qui les rend (`GET /services/{key}/spec`,
//! HRT-87) les rendra sous [`crate::api::services::Concealed`].
//!
//! Cette tranche déclare le type de l'écart ; la route et la vue complète viennent avec leur ticket.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::deploy::{DeployMount, DeployPlace, DeployPort, DeploySpec};

/// L'écart entre ce qui est VOULU (la description) et ce qui est OBSERVÉ (le moteur). Il est affiché,
/// jamais corrigé seul (BR-EDIT-024).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftView {
    /// Le conteneur est celui de la description.
    None,
    /// « Modifié hors de Hearth » : le conteneur existe mais ne correspond plus à la description.
    ModifiedOutside,
    /// « Conteneur absent » : la description existe, le conteneur non. Seul geste possible : « Recréer le
    /// service ».
    ContainerMissing,
    /// Un écart que ce client ne connaît pas.
    #[serde(other)]
    Unknown,
}

/// L'image et sa version telles que l'acte les signe et que l'agent les déploie : `(dépôt, version)`
/// (BR-DEPLOY-005). La version écrite dans le champ de l'image (`nginx:1.27`, `registry:5000/app:v2`) passe
/// dans le champ Version ; une version vide vaut `latest`. **Lecture tolérante** : un texte qui n'est pas une
/// image valide est rendu tel quel, c'est l'agent qui le refuse (avec le champ en faute) après la
/// confirmation. Source unique pour l'agent (qui reconstruit l'acte) et la liaison (qui le signe).
pub fn split_image(image: &str, tag: Option<&str>) -> (String, String) {
    let image = image.trim();
    // La version suit le dernier « : » situé après le dernier « / » (un « : » avant est un port de registre).
    let after_slash = image.rfind('/').map_or(0, |i| i + 1);
    let (repository, embedded) = match image[after_slash..].rfind(':') {
        Some(i) => {
            let cut = after_slash + i;
            (&image[..cut], Some(&image[cut + 1..]))
        }
        None => (image, None),
    };
    let tag = embedded
        .filter(|tag| !tag.is_empty())
        .or_else(|| tag.map(str::trim).filter(|tag| !tag.is_empty()))
        .unwrap_or("latest");
    (repository.to_owned(), tag.to_owned())
}

/// `dépôt:version` : le paramètre `image` de l'acte `ServiceDeploy`.
pub fn deploy_image_ref(image: &str, tag: Option<&str>) -> String {
    let (repository, tag) = split_image(image, tag);
    format!("{repository}:{tag}")
}

fn put(hasher: &mut Sha256, bytes: &[u8]) {
    let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    hasher.update(length.to_be_bytes());
    hasher.update(&bytes[..length as usize]);
}

/// L'empreinte de la partie NON secrète d'un déploiement : SHA-256 d'un encodage à longueurs préfixées du nom,
/// de l'image, de la version, des ports triés, des montages triés, des limites et des cases cochées. Une
/// empreinte plutôt que la liste : vingt chemins de 4 096 caractères dépassent la borne d'un paramètre signé.
/// **Jamais** une valeur de variable, un nom de variable ni la commande (une signature observée ne doit pas
/// permettre de tester un secret hors ligne, ADR-0031) : seul leur nombre est signé, à part.
/// Indépendante de l'ordre des ports et des montages. Source unique (agent et liaison).
pub fn deploy_digest(spec: &DeploySpec) -> [u8; 32] {
    let mut hasher = Sha256::new();
    put(&mut hasher, b"hearth/deploy-digest/1");
    put(&mut hasher, spec.name.as_bytes());
    let (repository, tag) = split_image(&spec.image, spec.tag.as_deref());
    put(&mut hasher, repository.as_bytes());
    put(&mut hasher, tag.as_bytes());
    let mut ports: Vec<DeployPort> = spec.ports.clone();
    ports.sort_unstable();
    hasher.update(u32::try_from(ports.len()).unwrap_or(u32::MAX).to_be_bytes());
    for port in ports {
        hasher.update(port.host_port.to_be_bytes());
        hasher.update(port.container_port.to_be_bytes());
        put(&mut hasher, port.protocol.as_str().as_bytes());
    }
    let mut mounts: Vec<&DeployMount> = spec.mounts.iter().collect();
    mounts.sort_unstable();
    hasher.update(
        u32::try_from(mounts.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    for mount in mounts {
        put(&mut hasher, mount.target.as_bytes());
        put(
            &mut hasher,
            match mount.place {
                DeployPlace::Managed => b"managed",
                DeployPlace::Host => b"host",
            },
        );
        put(
            &mut hasher,
            mount.host_path.as_deref().unwrap_or("").as_bytes(),
        );
        hasher.update([
            u8::from(mount.read_only),
            u8::from(mount.create),
            u8::from(mount.confirmed),
        ]);
    }
    // Les limites et les cases cochées : un message rejoué avec d'autres limites ne vaut plus.
    match spec.resources.cpu_millis {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.to_be_bytes());
        }
        None => hasher.update([0]),
    }
    match spec.resources.memory_bytes {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.to_be_bytes());
        }
        None => hasher.update([0]),
    }
    hasher.update([
        u8::from(spec.accept.reserved_ports),
        u8::from(spec.accept.low_disk),
    ]);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drift_has_the_three_wire_names_and_tolerates_a_fourth() {
        for (drift, name) in [
            (DriftView::None, "none"),
            (DriftView::ModifiedOutside, "modified_outside"),
            (DriftView::ContainerMissing, "container_missing"),
        ] {
            assert_eq!(serde_json::to_value(drift).expect("json"), name);
        }
        let read: DriftView = serde_json::from_value(serde_json::json!("brand_new")).expect("lu");
        assert_eq!(read, DriftView::Unknown);
    }

    fn sample() -> DeploySpec {
        serde_json::from_value(serde_json::json!({
            "name": "web", "image": "nginx:1.27",
            "env": [{"name": "A", "value": "SECRET-1"}],
            "command": "run --x",
            "ports": [
                {"host_port": 8080, "container_port": 80},
                {"host_port": 53, "container_port": 53, "protocol": "udp"}
            ],
            "mounts": [{"target": "/data"}, {"target": "/conf", "read_only": true}],
            "resources": {"cpu_millis": 1000, "memory_bytes": 268435456}
        }))
        .expect("spec")
    }

    #[test]
    fn the_image_version_moves_into_the_version_field_and_empty_means_latest() {
        assert_eq!(
            split_image("nginx:1.27", None),
            ("nginx".into(), "1.27".into())
        );
        assert_eq!(
            split_image("nginx", Some("1.0")),
            ("nginx".into(), "1.0".into())
        );
        assert_eq!(
            split_image("nginx", None),
            ("nginx".into(), "latest".into())
        );
        assert_eq!(
            split_image("nginx", Some(" ")),
            ("nginx".into(), "latest".into())
        );
        // Le port d'un registre n'est pas une version.
        assert_eq!(
            split_image("registry:5000/team/app", None),
            ("registry:5000/team/app".into(), "latest".into())
        );
        assert_eq!(
            split_image("registry:5000/team/app:v2", None),
            ("registry:5000/team/app".into(), "v2".into())
        );
        assert_eq!(deploy_image_ref("nginx", Some("1.0")), "nginx:1.0");
    }

    #[test]
    fn the_digest_is_stable_and_independent_of_the_order_of_ports_and_mounts() {
        let a = sample();
        let mut b = sample();
        b.ports.reverse();
        b.mounts.reverse();
        assert_eq!(deploy_digest(&a), deploy_digest(&b));
        // Même empreinte que la forme équivalente (version dans le champ Version).
        let mut c = sample();
        c.image = "nginx".into();
        c.tag = Some("1.27".into());
        assert_eq!(deploy_digest(&a), deploy_digest(&c));
    }

    #[test]
    fn the_digest_moves_with_every_non_secret_part_and_ignores_every_secret() {
        let base = deploy_digest(&sample());
        let changed = |change: &dyn Fn(&mut DeploySpec)| {
            let mut spec = sample();
            change(&mut spec);
            deploy_digest(&spec)
        };
        assert_ne!(base, changed(&|s| s.name = "web2".into()));
        assert_ne!(base, changed(&|s| s.image = "nginx:1.28".into()));
        assert_ne!(base, changed(&|s| s.ports[0].host_port = 8081));
        assert_ne!(base, changed(&|s| s.mounts[0].read_only = true));
        assert_ne!(base, changed(&|s| s.resources.memory_bytes = Some(1)));
        assert_ne!(base, changed(&|s| s.accept.low_disk = true));
        // Les secrets ne comptent pas : ni la valeur, ni le nom de la variable, ni la commande.
        assert_eq!(base, changed(&|s| s.env[0].value = "AUTRE".into()));
        assert_eq!(base, changed(&|s| s.env[0].name = "B".into()));
        assert_eq!(base, changed(&|s| s.command = None));
    }
}
