//! Le nom d'une image tel que l'affiche la liste (`BR-SVC-002`, `BR-SVC-022`).

/// Image d'un service : nom sans version, version, identifiant court.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageName {
    /// Absent pour une image sans nom (le conteneur désigne l'image par son identifiant).
    pub reference: Option<String>,
    pub tag: Option<String>,
    /// 12 caractères hexadécimaux au plus de l'identifiant de l'image.
    pub short_id: String,
}

/// Découpe la référence de création d'un conteneur (`nginx:1.27`, `registry:5000/ns/app`,
/// `app@sha256:…`) et l'identifiant de l'image. Une référence qui n'est que l'identifiant de
/// l'image (ou vide) donne une image sans nom (`BR-SVC-022`).
pub fn image_name(created_from: &str, image_id: &str) -> ImageName {
    let short_id = short_image_id(image_id);
    let trimmed = created_from.trim();
    let id_like = trimmed.is_empty()
        || trimmed.starts_with("sha256:")
        || (trimmed.len() >= 12
            && trimmed.len() <= 64
            && trimmed.chars().all(|c| c.is_ascii_hexdigit()));
    if id_like {
        return ImageName {
            reference: None,
            tag: None,
            short_id,
        };
    }
    // Un condensat (`@sha256:…`) n'est pas une version : on le garde dans le nom.
    let (name_part, digest) = match trimmed.split_once('@') {
        Some((name, digest)) => (name, Some(digest)),
        None => (trimmed, None),
    };
    // La version suit le dernier « : » situé après le dernier « / » (un « : » avant est un port).
    let after_slash = name_part.rfind('/').map_or(0, |i| i + 1);
    let (name, tag) = match name_part[after_slash..].rfind(':') {
        Some(i) => {
            let cut = after_slash + i;
            (&name_part[..cut], Some(name_part[cut + 1..].to_owned()))
        }
        None => (name_part, None),
    };
    let reference = match digest {
        Some(digest) => format!("{name}@{digest}"),
        None => name.to_owned(),
    };
    ImageName {
        reference: Some(reference),
        tag,
        short_id,
    }
}

fn short_image_id(image_id: &str) -> String {
    image_id
        .strip_prefix("sha256:")
        .unwrap_or(image_id)
        .chars()
        .take(12)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn a_name_and_a_version_are_split() {
        let image = image_name("nginx:1.27", ID);
        assert_eq!(image.reference.as_deref(), Some("nginx"));
        assert_eq!(image.tag.as_deref(), Some("1.27"));
        assert_eq!(image.short_id, "0123456789ab");
    }

    #[test]
    fn a_registry_port_is_not_a_version() {
        let image = image_name("registry:5000/team/app", ID);
        assert_eq!(image.reference.as_deref(), Some("registry:5000/team/app"));
        assert_eq!(image.tag, None);
        let image = image_name("registry:5000/team/app:v2", ID);
        assert_eq!(image.reference.as_deref(), Some("registry:5000/team/app"));
        assert_eq!(image.tag.as_deref(), Some("v2"));
    }

    #[test]
    fn a_digest_stays_in_the_name() {
        let image = image_name("app@sha256:abc", ID);
        assert_eq!(image.reference.as_deref(), Some("app@sha256:abc"));
        assert_eq!(image.tag, None);
    }

    #[test]
    fn an_image_known_only_by_its_id_has_no_name() {
        for created_from in [ID, "0123456789abcdef", "", "  "] {
            let image = image_name(created_from, ID);
            assert_eq!(image.reference, None, "{created_from:?}");
            assert_eq!(image.tag, None);
            assert_eq!(image.short_id, "0123456789ab");
        }
    }
}
