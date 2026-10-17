//! Mémoire recommandée et crans de mémoire (HRT-122 ; BR-RES-031 à 033, BR-RES-038). Chaque fiche piégée est
//! dérivée de la fiche de base par UNE modification et doit être refusée avec LE bon motif typé.

use super::Fiche;
use super::loading::error::{FicheRefusee, Motif};
use super::loading::memory::{RecommendedGap, ServedTier, TierId, recommended_gap};
use super::loading::schema::fiche::{Evidence, PlainEvidence};
use super::test_support::*;

const UNMEASURED: &str = "memory = { evidence = \"unmeasured\" }";
const GB: u64 = 1024 * 1024 * 1024;
const MEASURED: &str = "\"measured\"";
const PUBLISHER: &str = "{ publisher = \"https://example.org/req\" }";

fn memory_line(min: Option<u64>, rec: Option<u64>, evidence: &str) -> String {
    let mut parts = Vec::new();
    if let Some(m) = min {
        parts.push(format!("min_bytes = {m}"));
    }
    if let Some(r) = rec {
        parts.push(format!("recommended_bytes = {r}"));
    }
    parts.push(format!("evidence = {evidence}"));
    format!("memory = {{ {} }}", parts.join(", "))
}

/// La fiche de base avec une mémoire, et des blocs de crans ajoutés à la fin de `[resources]`.
fn with_memory(memory: &str, extra: &str) -> String {
    let text = replace(BASE_FICHE, UNMEASURED, memory);
    let disk = "disk = { evidence = \"unmeasured\" }";
    replace(&text, disk, &format!("{disk}\n\n{extra}"))
}

fn published(text: &str) -> String {
    let digest = format!("sha256:{}", "a".repeat(64));
    replace(
        &replace(text, "status = \"preview\"", "status = \"published\""),
        "tag = \"1.2.3\"",
        &format!("tag = \"1.2.3\"\ndigest = \"{digest}\""),
    )
}

fn load_ok(text: &str) -> Fiche {
    load_texts(text, BASE_SETTINGS).expect("la fiche est valide")
}

fn refused(text: &str) -> FicheRefusee {
    load_texts(text, BASE_SETTINGS).expect_err("la fiche piégée a été acceptée")
}

fn tier(id: &str, from: u64, evidence: &str) -> String {
    format!(
        "[[resources.memory_tier]]\nid = \"{id}\"\nfrom_bytes = {from}\nevidence = {evidence}\n"
    )
}

fn ids(tiers: &[ServedTier]) -> Vec<&'static str> {
    tiers.iter().map(|t| t.id.as_str()).collect()
}

// ---- Compatibilité ------------------------------------------------------------------------------------------

#[test]
fn a_fiche_without_tier_block_loads_as_before_and_serves_no_tier() {
    let fiche = base();
    assert!(fiche.file.resources.memory_tier.is_empty());
    assert!(fiche.file.resources.tiers.is_none());
    assert!(fiche.memory_tiers().is_empty());
    assert!(rich().file.resources.memory_tier.is_empty());
}

// ---- Fiches piégées ------------------------------------------------------------------------------------------

#[test]
fn declared_tiers_with_a_bad_identifier_are_refused() {
    let mem = memory_line(None, Some(2 * GB), MEASURED);
    for (id, expected) in [
        ("giant", Motif::CranInconnu { id: "giant".into() }),
        ("slow", Motif::CranNonDeclarable { id: "slow".into() }),
        (
            "recommended",
            Motif::CranNonDeclarable {
                id: "recommended".into(),
            },
        ),
        (
            "unstable",
            Motif::CranNonDeclarable {
                id: "unstable".into(),
            },
        ),
    ] {
        let text = with_memory(&mem, &tier(id, 4 * GB, MEASURED));
        assert_eq!(refused(&text).motif, expected, "{id}");
    }
    let twice = format!(
        "{}{}",
        tier("smooth", 3 * GB, MEASURED),
        tier("smooth", 4 * GB, MEASURED)
    );
    assert_eq!(
        refused(&with_memory(&mem, &twice)).motif,
        Motif::CranEnDouble {
            id: "smooth".into()
        }
    );
}

#[test]
fn from_bytes_must_rise_strictly_above_the_recommended_memory() {
    let mem = memory_line(None, Some(2 * GB), MEASURED);
    let culprit = |text: String| match refused(&text).motif {
        Motif::CranBorneInvalide { id, .. } => id,
        other => panic!("motif inattendu : {other:?}"),
    };
    // Pas au-dessus de la recommandée (égal, puis en dessous).
    assert_eq!(
        culprit(with_memory(&mem, &tier("smooth", 2 * GB, MEASURED))),
        "smooth"
    );
    assert_eq!(
        culprit(with_memory(&mem, &tier("smooth", GB, MEASURED))),
        "smooth"
    );
    // Non croissants, quel que soit l'ordre d'écriture dans le fichier.
    let flat = format!(
        "{}{}",
        tier("ultra", 4 * GB, MEASURED),
        tier("smooth", 4 * GB, MEASURED)
    );
    assert_eq!(culprit(with_memory(&mem, &flat)), "ultra");
    // Le refus situe le cran par sa place dans le fichier, comme les autres refus de crans.
    assert_eq!(
        refused(&with_memory(&mem, &flat)).chemin,
        "resources.memory_tier[0]"
    );
    let inverted = format!(
        "{}{}",
        tier("community", 3 * GB, MEASURED),
        tier("ultra", 6 * GB, MEASURED)
    );
    assert_eq!(culprit(with_memory(&mem, &inverted)), "community");
    // Sans mémoire recommandée, aucun cran ne peut se placer.
    let none = memory_line(None, None, MEASURED);
    assert_eq!(
        culprit(with_memory(&none, &tier("smooth", 4 * GB, MEASURED))),
        "smooth"
    );
}

#[test]
fn a_tier_refuses_idle_and_unmeasured_proof() {
    let mem = memory_line(None, Some(2 * GB), MEASURED);
    for proof in ["\"measured_idle\"", "\"unmeasured\""] {
        assert_eq!(
            refused(&with_memory(&mem, &tier("ultra", 8 * GB, proof))).motif,
            Motif::CranPreuveInterdite { id: "ultra".into() },
            "{proof}"
        );
    }
}

#[test]
fn a_publisher_tier_needs_an_https_address() {
    let mem = memory_line(None, Some(2 * GB), MEASURED);
    assert_eq!(
        refused(&with_memory(
            &mem,
            &tier(
                "smooth",
                4 * GB,
                "{ publisher = \"http://example.org/req\" }"
            )
        ))
        .motif,
        Motif::AdresseNonHttps
    );
    // Adresse absente : `publisher` est obligatoire, et aucun autre champ n'existe.
    let missing = refused(&with_memory(&mem, &tier("smooth", 4 * GB, "{ }"))).motif;
    assert!(
        matches!(
            missing,
            Motif::VarianteInconnue | Motif::ChampManquant { .. }
        ),
        "{missing:?}"
    );
    load_ok(&with_memory(&mem, &tier("smooth", 4 * GB, PUBLISHER)));
}

#[test]
fn a_tier_has_no_free_text_field() {
    let mem = memory_line(None, Some(2 * GB), MEASURED);
    for field in ["label", "text", "players", "url", "help"] {
        let tier = format!("{}{field} = \"x\"\n", tier("smooth", 4 * GB, MEASURED));
        assert_eq!(
            refused(&with_memory(&mem, &tier)).motif,
            Motif::ChampInconnu {
                champ: field.into()
            },
            "{field}"
        );
    }
}

#[test]
fn hidden_lists_the_six_identifiers_only_once_each() {
    let mem = memory_line(None, Some(2 * GB), MEASURED);
    load_ok(&with_memory(
        &mem,
        "[resources.tiers]\nhidden = [\"community\", \"ultra\"]\n",
    ));
    assert_eq!(
        refused(&with_memory(
            &mem,
            "[resources.tiers]\nhidden = [\"giant\"]\n"
        ))
        .motif,
        Motif::CranInconnu { id: "giant".into() }
    );
    assert_eq!(
        refused(&with_memory(
            &mem,
            "[resources.tiers]\nhidden = [\"ultra\", \"ultra\"]\n"
        ))
        .motif,
        Motif::CranEnDouble { id: "ultra".into() }
    );
    let twice = refused(&with_memory(
        &mem,
        "[resources.tiers]\nhidden = [\"ultra\", \"ultra\"]\n",
    ));
    assert_eq!(twice.chemin, "resources.tiers.hidden[1]");
}

// ---- Règle de publication (BR-RES-038) -------------------------------------------------------------------------

#[test]
fn a_published_fiche_needs_a_founded_recommended_memory() {
    let gap =
        |memory: &str| match refused(&published(&replace(BASE_FICHE, UNMEASURED, memory))).motif {
            Motif::PublieeSansMemoireRecommandee { raison } => raison,
            other => panic!("motif inattendu : {other:?}"),
        };
    assert!(gap(UNMEASURED).contains("manque"));
    // Un plancher sans recommandation ne suffit pas.
    assert!(gap(&memory_line(Some(GB), None, MEASURED)).contains("manque"));
    // Une mesure au repos, même avec une recommandée, ne suffit pas.
    assert!(gap(&memory_line(Some(GB), Some(2 * GB), "\"measured_idle\"")).contains("au repos"));
    // Mesurée, ou chiffre de l'éditeur : publiée.
    for evidence in [MEASURED, PUBLISHER] {
        let memory = memory_line(None, Some(2 * GB), evidence);
        load_ok(&published(&replace(BASE_FICHE, UNMEASURED, &memory)));
    }
}

#[test]
fn a_preview_fiche_stays_loadable_without_a_recommended_memory() {
    let fiche = load_ok(BASE_FICHE);
    assert_eq!(recommended_gap(&fiche.file), Some(RecommendedGap::Missing));
    let idle_line = memory_line(Some(GB), Some(2 * GB), "\"measured_idle\"");
    let idle = load_ok(&replace(BASE_FICHE, UNMEASURED, &idle_line));
    assert_eq!(recommended_gap(&idle.file), Some(RecommendedGap::IdleOnly));
}

// ---- Ce qui est servi ----------------------------------------------------------------------------------------------

#[test]
fn only_the_recommended_tier_is_served_when_only_it_is_declared() {
    let fiche = load_ok(&with_memory(&memory_line(None, Some(2 * GB), MEASURED), ""));
    let tiers = fiche.memory_tiers();
    assert_eq!(ids(&tiers), ["recommended"]);
    assert_eq!(tiers[0].from_bytes, Some(2 * GB));
    assert_eq!(tiers[0].evidence, Evidence::Plain(PlainEvidence::Measured));
}

#[test]
fn unstable_needs_a_measured_minimum_and_slow_needs_a_minimum() {
    // Minimum annoncé par l'éditeur : « Lent » oui, « Instable » non.
    let publisher = load_ok(&with_memory(
        &memory_line(Some(GB), Some(2 * GB), PUBLISHER),
        "",
    ));
    assert_eq!(ids(&publisher.memory_tiers()), ["slow", "recommended"]);
    // Minimum mesuré : les trois.
    let measured = load_ok(&with_memory(
        &memory_line(Some(GB), Some(2 * GB), MEASURED),
        "",
    ));
    let tiers = measured.memory_tiers();
    assert_eq!(ids(&tiers), ["unstable", "slow", "recommended"]);
    assert_eq!(
        (tiers[0].from_bytes, tiers[0].below_bytes),
        (None, Some(GB))
    );
    assert_eq!(
        (tiers[1].from_bytes, tiers[1].below_bytes),
        (Some(GB), Some(2 * GB))
    );
    // Mesuré au repos : « Instable » oui (un plancher vu), « Recommandé » non (pas fondée).
    let idle_line = memory_line(Some(GB), Some(2 * GB), "\"measured_idle\"");
    let idle = load_ok(&with_memory(&idle_line, ""));
    assert_eq!(ids(&idle.memory_tiers()), ["unstable", "slow"]);
    // Sans minimum : ni « Instable » ni « Lent ».
    let none = load_ok(&with_memory(&memory_line(None, Some(2 * GB), MEASURED), ""));
    assert_eq!(ids(&none.memory_tiers()), ["recommended"]);
}

#[test]
fn hidden_tiers_are_never_served_even_with_a_number() {
    let extra = format!(
        "{}{}[resources.tiers]\nhidden = [\"community\", \"slow\"]\n",
        tier("smooth", 4 * GB, MEASURED),
        tier("community", 16 * GB, PUBLISHER)
    );
    let memory = memory_line(Some(GB), Some(2 * GB), MEASURED);
    let fiche = load_ok(&with_memory(&memory, &extra));
    assert_eq!(
        ids(&fiche.memory_tiers()),
        ["unstable", "recommended", "smooth"]
    );
}

#[test]
fn no_tier_figure_is_ever_derived_from_another() {
    // Fiche avec la seule mémoire recommandée : aucun cran du haut n'apparaît, ni 2x ni 4x.
    let fiche = load_ok(&with_memory(&memory_line(None, Some(2 * GB), MEASURED), ""));
    for t in fiche.memory_tiers() {
        assert!(
            !matches!(t.id, TierId::Smooth | TierId::Ultra | TierId::Community),
            "{:?}",
            t.id
        );
    }
    // Chaque nombre servi est écrit dans la fiche.
    let extra = format!(
        "{}{}",
        tier("smooth", 3 * GB, MEASURED),
        tier("ultra", 5 * GB, PUBLISHER)
    );
    let memory = memory_line(Some(GB), Some(2 * GB), MEASURED);
    let fiche = load_ok(&with_memory(&memory, &extra));
    let written = [GB, 2 * GB, 3 * GB, 5 * GB];
    let served = fiche.memory_tiers();
    for t in &served {
        for n in [t.from_bytes, t.below_bytes].into_iter().flatten() {
            assert!(
                written.contains(&n),
                "{:?} : {n} n'est pas écrit dans la fiche",
                t.id
            );
        }
    }
    let ultra = served
        .iter()
        .find(|t| t.id == TierId::Ultra)
        .expect("ultra");
    assert!(
        matches!(&ultra.evidence, Evidence::Publisher(p) if p.publisher == "https://example.org/req")
    );
}
