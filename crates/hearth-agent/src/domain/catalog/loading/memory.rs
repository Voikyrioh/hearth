//! Mémoire recommandée d'une fiche et crans de mémoire (HRT-122 ; BR-RES-031 à 033, 038, ADR-0050).
//!
//! Domaine pur, autonome (`super::` seulement). Trois crans se DÉDUISENT de `min_bytes` et `recommended_bytes`
//! (« Instable », « Lent », « Recommandé ») ; trois se DÉCLARENT (`[[resources.memory_tier]]` : « Fluide »,
//! « Ultra », « Communautaire »). Un chiffre de cran n'est jamais calculé à partir d'un autre (BR-RES-031).

use super::schema::fiche::{Evidence, FicheFile, PlainEvidence};

/// Les six crans, dans l'ordre où ils sont montrés.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TierId {
    Unstable,
    Slow,
    Recommended,
    Smooth,
    Ultra,
    Community,
}

impl TierId {
    pub const ALL: [TierId; 6] = [
        Self::Unstable,
        Self::Slow,
        Self::Recommended,
        Self::Smooth,
        Self::Ultra,
        Self::Community,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unstable => "unstable",
            Self::Slow => "slow",
            Self::Recommended => "recommended",
            Self::Smooth => "smooth",
            Self::Ultra => "ultra",
            Self::Community => "community",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == id)
    }

    /// Seuls « Fluide », « Ultra » et « Communautaire » s'écrivent dans une fiche.
    pub fn is_declarable(self) -> bool {
        matches!(self, Self::Smooth | Self::Ultra | Self::Community)
    }
}

/// Un cran servi au client : sa limite, et la preuve du chiffre dont elle vient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedTier {
    pub id: TierId,
    /// Début du cran. `None` pour « Instable » (tout ce qui est sous la mémoire minimale).
    pub from_bytes: Option<u64>,
    /// Fin exclusive du cran quand elle vient d'un chiffre de la fiche : « Instable » finit à la mémoire
    /// minimale, « Lent » à la mémoire recommandée. `None` sinon.
    pub below_bytes: Option<u64>,
    pub evidence: Evidence,
}

/// Pourquoi la mémoire recommandée d'une fiche ne la laisse pas publier (BR-RES-038).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecommendedGap {
    /// Aucun chiffre de mémoire recommandée.
    Missing,
    /// Un chiffre, mais seulement mesuré au repos : un plancher, pas une recommandation.
    IdleOnly,
}

impl RecommendedGap {
    pub fn explication(self) -> &'static str {
        match self {
            Self::Missing => "la mémoire recommandée manque",
            Self::IdleOnly => "la mémoire recommandée n'a qu'une mesure au repos",
        }
    }
}

/// La mémoire recommandée est fondée si elle existe avec une preuve `measured` ou `publisher`.
pub fn recommended_gap(file: &FicheFile) -> Option<RecommendedGap> {
    let memory = &file.resources.memory;
    if memory.recommended_bytes.is_none() {
        return Some(RecommendedGap::Missing);
    }
    match memory.evidence {
        Evidence::Publisher(_) | Evidence::Plain(PlainEvidence::Measured) => None,
        Evidence::Plain(PlainEvidence::MeasuredIdle) => Some(RecommendedGap::IdleOnly),
        Evidence::Plain(PlainEvidence::Unmeasured) => Some(RecommendedGap::Missing),
    }
}

/// Les crans que la fiche donne au client, dans l'ordre d'affichage. Un cran sans chiffre fondé, ou listé dans
/// `[resources.tiers] hidden`, n'est jamais servi (BR-RES-031, BR-RES-032, BR-RES-033).
pub fn memory_tiers(file: &FicheFile) -> Vec<ServedTier> {
    let memory = &file.resources.memory;
    let hidden: Vec<TierId> = file
        .resources
        .tiers
        .as_ref()
        .map(|t| t.hidden.iter().filter_map(|id| TierId::parse(id)).collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    let min_measured = matches!(
        memory.evidence,
        Evidence::Plain(PlainEvidence::Measured | PlainEvidence::MeasuredIdle)
    );
    if let (Some(min), true) = (memory.min_bytes, min_measured) {
        out.push(ServedTier {
            id: TierId::Unstable,
            from_bytes: None,
            below_bytes: Some(min),
            evidence: memory.evidence.clone(),
        });
    }
    if let Some(min) = memory.min_bytes {
        out.push(ServedTier {
            id: TierId::Slow,
            from_bytes: Some(min),
            below_bytes: memory.recommended_bytes,
            evidence: memory.evidence.clone(),
        });
    }
    if let (Some(rec), None) = (memory.recommended_bytes, recommended_gap(file)) {
        out.push(ServedTier {
            id: TierId::Recommended,
            from_bytes: Some(rec),
            below_bytes: None,
            evidence: memory.evidence.clone(),
        });
    }
    for tier in &file.resources.memory_tier {
        if let Some(id) = TierId::parse(&tier.id) {
            out.push(ServedTier {
                id,
                from_bytes: Some(tier.from_bytes),
                below_bytes: None,
                evidence: tier.evidence.clone(),
            });
        }
    }
    out.retain(|t| !hidden.contains(&t.id));
    out.sort_by_key(|t| t.id);
    out
}
