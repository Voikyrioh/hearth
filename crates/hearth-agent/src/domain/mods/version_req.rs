//! Les versions de Minecraft et les plages que les mods en disent (HRT-254, BR-MOD-021) : domaine pur.
//!
//! Une version du jeu est une suite de nombres (`1.20.1`, `26.3`). Tout ce qui n'a pas cette forme (instantané
//! `24w14a`, version avec lettres) n'est PAS comprise : le jugement est alors « non vérifié », jamais un refus.
//! Les plages viennent d'un fichier HOSTILE : une plage mal formée donne `None`, jamais une panique ni une valeur
//! devinée. Deux grammaires : les prédicats de Fabric et de Quilt, les plages Maven de Forge et de NeoForge.

use std::cmp::Ordering;

/// Au plus ce nombre de composants d'une version (borne contre un texte hostile).
const MAX_PARTS: usize = 6;
/// Au plus ce nombre de prédicats ou d'intervalles dans une plage.
const MAX_TERMS: usize = 32;
/// Longueur maximale d'une plage lue.
const MAX_REQ_LEN: usize = 256;

/// Une version du jeu : une suite de nombres.
#[derive(Debug, Clone)]
pub struct GameVersion(Vec<u64>);

impl PartialEq for GameVersion {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for GameVersion {}

impl GameVersion {
    /// `1.20.1`, `26.3` : des nombres séparés par des points, 6 composants au plus. Sinon `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() || text.len() > 32 {
            return None;
        }
        let mut parts = Vec::new();
        for piece in text.split('.') {
            if piece.is_empty() || piece.len() > 9 || !piece.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            parts.push(piece.parse::<u64>().ok()?);
            if parts.len() > MAX_PARTS {
                return None;
            }
        }
        Some(Self(parts))
    }
}

impl PartialOrd for GameVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GameVersion {
    /// `1.20` vaut `1.20.0` : le plus court est complété de zéros.
    fn cmp(&self, other: &Self) -> Ordering {
        let len = self.0.len().max(other.0.len());
        for i in 0..len {
            let a = self.0.get(i).copied().unwrap_or(0);
            let b = other.0.get(i).copied().unwrap_or(0);
            match a.cmp(&b) {
                Ordering::Equal => {}
                other => return other,
            }
        }
        Ordering::Equal
    }
}

/// Une version est-elle dans la plage ?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Containment {
    Yes,
    No,
    /// La plage ou la version n'est pas comprise : « non vérifié ».
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Bound {
    version: GameVersion,
    inclusive: bool,
}

/// Un intervalle de versions ; une borne absente est ouverte.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Interval {
    lo: Option<Bound>,
    hi: Option<Bound>,
}

impl Interval {
    fn contains(&self, v: &GameVersion) -> bool {
        let above = match &self.lo {
            None => true,
            Some(b) => match v.cmp(&b.version) {
                Ordering::Greater => true,
                Ordering::Equal => b.inclusive,
                Ordering::Less => false,
            },
        };
        let below = match &self.hi {
            None => true,
            Some(b) => match v.cmp(&b.version) {
                Ordering::Less => true,
                Ordering::Equal => b.inclusive,
                Ordering::Greater => false,
            },
        };
        above && below
    }

    /// L'intersection de deux intervalles (le ET des prédicats Fabric).
    fn and(self, other: Self) -> Self {
        let lo = match (self.lo, other.lo) {
            (Some(a), Some(b)) => Some(match a.version.cmp(&b.version) {
                Ordering::Greater => a,
                Ordering::Less => b,
                Ordering::Equal => Bound {
                    version: a.version,
                    inclusive: a.inclusive && b.inclusive,
                },
            }),
            (a, b) => a.or(b),
        };
        let hi = match (self.hi, other.hi) {
            (Some(a), Some(b)) => Some(match a.version.cmp(&b.version) {
                Ordering::Less => a,
                Ordering::Greater => b,
                Ordering::Equal => Bound {
                    version: a.version,
                    inclusive: a.inclusive && b.inclusive,
                },
            }),
            (a, b) => a.or(b),
        };
        Self { lo, hi }
    }
}

/// Une plage de versions du jeu : une union d'intervalles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionReq(Vec<Interval>);

impl VersionReq {
    /// La version est-elle dans la plage ? `Unknown` quand la version du serveur n'est pas une suite de nombres.
    pub fn contains(&self, version: Option<&GameVersion>) -> Containment {
        let Some(version) = version else {
            return Containment::Unknown;
        };
        if self.0.iter().any(|interval| interval.contains(version)) {
            Containment::Yes
        } else {
            Containment::No
        }
    }

    /// Les prédicats de Fabric et de Quilt. Plusieurs plages (tableau) = OU ; des prédicats séparés par des
    /// espaces = ET. Une plage non comprise : `None`.
    pub fn parse_fabric(ranges: &[&str]) -> Option<Self> {
        if ranges.is_empty() || ranges.len() > MAX_TERMS {
            return None;
        }
        let mut union = Vec::new();
        for range in ranges {
            if range.len() > MAX_REQ_LEN {
                return None;
            }
            let mut interval = Interval::default();
            let mut any = false;
            for predicate in range.split_whitespace() {
                interval = interval.and(fabric_predicate(predicate)?);
                any = true;
                if union.len() > MAX_TERMS {
                    return None;
                }
            }
            if !any {
                return None;
            }
            union.push(interval);
        }
        Some(Self(union))
    }

    /// Les plages Maven de Forge et de NeoForge : `[1.20.1,1.21)`, `[1.20.1]`, `(,1.21]`, une version nue = « au
    /// moins ». Plusieurs intervalles séparés par une virgule entre deux plages. Une plage non comprise : `None`.
    pub fn parse_maven(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() || text.len() > MAX_REQ_LEN {
            return None;
        }
        let bytes = text.as_bytes();
        if bytes[0] != b'[' && bytes[0] != b'(' {
            // Version nue : « au moins ».
            let version = GameVersion::parse(text)?;
            return Some(Self(vec![Interval {
                lo: Some(Bound {
                    version,
                    inclusive: true,
                }),
                hi: None,
            }]));
        }
        let mut union = Vec::new();
        let mut rest = text;
        while !rest.is_empty() {
            let open = rest.chars().next()?;
            let close = rest.find([']', ')'])?;
            let inner = &rest[1..close];
            let closing = rest[close..].chars().next()?;
            let inclusive_lo = open == '[';
            let inclusive_hi = closing == ']';
            let interval = match inner.split_once(',') {
                None => {
                    // `[1.20.1]` : exactement cette version.
                    if !(inclusive_lo && inclusive_hi) {
                        return None;
                    }
                    let version = GameVersion::parse(inner)?;
                    Interval {
                        lo: Some(Bound {
                            version: version.clone(),
                            inclusive: true,
                        }),
                        hi: Some(Bound {
                            version,
                            inclusive: true,
                        }),
                    }
                }
                Some((lo, hi)) => {
                    let lo = lo.trim();
                    let hi = hi.trim();
                    if hi.contains(',') {
                        return None;
                    }
                    Interval {
                        lo: if lo.is_empty() {
                            None
                        } else {
                            Some(Bound {
                                version: GameVersion::parse(lo)?,
                                inclusive: inclusive_lo,
                            })
                        },
                        hi: if hi.is_empty() {
                            None
                        } else {
                            Some(Bound {
                                version: GameVersion::parse(hi)?,
                                inclusive: inclusive_hi,
                            })
                        },
                    }
                }
            };
            union.push(interval);
            if union.len() > MAX_TERMS {
                return None;
            }
            rest = rest[close + 1..].trim_start();
            if let Some(after) = rest.strip_prefix(',') {
                rest = after.trim_start();
                if rest.is_empty() {
                    return None;
                }
            } else if !rest.is_empty() {
                return None;
            }
        }
        if union.is_empty() {
            None
        } else {
            Some(Self(union))
        }
    }
}

fn bound(version: GameVersion, inclusive: bool) -> Option<Bound> {
    Some(Bound { version, inclusive })
}

/// La version suivante au rang `rank` (0 = majeure) : `1.20.1` au rang 1 donne `1.21`.
fn bump(parts: &[u64], rank: usize) -> Option<GameVersion> {
    let mut next: Vec<u64> = parts.iter().copied().take(rank + 1).collect();
    while next.len() < rank + 1 {
        next.push(0);
    }
    *next.get_mut(rank)? = next.get(rank)?.checked_add(1)?;
    Some(GameVersion(next))
}

/// Un prédicat de Fabric : `*`, `1.20.x`, `1.20.*`, `>=1.20`, `>1.20`, `<=`, `<`, `=1.20`, `~1.20.1`, `^1.20`, ou
/// une version nue (exactement celle-ci).
fn fabric_predicate(predicate: &str) -> Option<Interval> {
    if predicate == "*" || predicate.eq_ignore_ascii_case("x") {
        return Some(Interval::default());
    }
    for (prefix, lo_incl, is_lo) in [(">=", true, true), ("<=", true, false)] {
        if let Some(rest) = predicate.strip_prefix(prefix) {
            let v = GameVersion::parse(rest)?;
            return Some(if is_lo {
                Interval {
                    lo: bound(v, lo_incl),
                    hi: None,
                }
            } else {
                Interval {
                    lo: None,
                    hi: bound(v, lo_incl),
                }
            });
        }
    }
    if let Some(rest) = predicate.strip_prefix('>') {
        return Some(Interval {
            lo: bound(GameVersion::parse(rest)?, false),
            hi: None,
        });
    }
    if let Some(rest) = predicate.strip_prefix('<') {
        return Some(Interval {
            lo: None,
            hi: bound(GameVersion::parse(rest)?, false),
        });
    }
    if let Some(rest) = predicate.strip_prefix('=') {
        let v = GameVersion::parse(rest)?;
        return Some(Interval {
            lo: bound(v.clone(), true),
            hi: bound(v, true),
        });
    }
    if let Some(rest) = predicate.strip_prefix('~') {
        // `~1.20.1` : au moins 1.20.1, avant 1.21 ; `~1` : avant 2.
        let v = GameVersion::parse(rest)?;
        let rank = if v.0.len() >= 2 { 1 } else { 0 };
        let hi = bump(&v.0, rank)?;
        return Some(Interval {
            lo: bound(v, true),
            hi: bound(hi, false),
        });
    }
    if let Some(rest) = predicate.strip_prefix('^') {
        // `^1.20` : au moins 1.20, avant 2.
        let v = GameVersion::parse(rest)?;
        let hi = bump(&v.0, 0)?;
        return Some(Interval {
            lo: bound(v, true),
            hi: bound(hi, false),
        });
    }
    // Un caractère générique : `1.20.x`, `1.x`, `1.20.*`.
    let trimmed = predicate
        .strip_suffix(".x")
        .or_else(|| predicate.strip_suffix(".X"))
        .or_else(|| predicate.strip_suffix(".*"));
    if let Some(prefix) = trimmed {
        let v = GameVersion::parse(prefix)?;
        let hi = bump(&v.0, v.0.len() - 1)?;
        return Some(Interval {
            lo: bound(v, true),
            hi: bound(hi, false),
        });
    }
    // Une version nue : exactement celle-ci.
    let v = GameVersion::parse(predicate)?;
    Some(Interval {
        lo: bound(v.clone(), true),
        hi: bound(v, true),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> GameVersion {
        GameVersion::parse(text).expect("version")
    }

    fn fabric(ranges: &[&str], server: &str) -> Containment {
        VersionReq::parse_fabric(ranges)
            .expect("plage")
            .contains(GameVersion::parse(server).as_ref())
    }

    fn maven(range: &str, server: &str) -> Containment {
        VersionReq::parse_maven(range)
            .expect("plage")
            .contains(GameVersion::parse(server).as_ref())
    }

    #[test]
    fn a_game_version_is_a_run_of_numbers_and_nothing_else() {
        assert_eq!(v("1.20.1"), v("1.20.1"));
        assert_eq!(v("26.3"), v("26.3.0"));
        assert!(v("1.20.1") > v("1.20"));
        assert!(v("26.3") > v("1.21.9"));
        for bad in [
            "",
            "24w14a",
            "1.20.x",
            "1..2",
            "a",
            "1.2.3.4.5.6.7",
            "-1",
            "1.2 ",
            "1e3",
        ] {
            // L'espace final est rogné ; les autres formes sont refusées.
            if bad == "1.2 " {
                assert!(GameVersion::parse(bad).is_some());
            } else {
                assert!(GameVersion::parse(bad).is_none(), "{bad:?}");
            }
        }
        assert!(GameVersion::parse(&"9".repeat(40)).is_none());
    }

    #[test]
    fn fabric_predicates_in_a_table() {
        use Containment::*;
        let table: &[(&[&str], &str, Containment)] = &[
            (&["*"], "1.20.1", Yes),
            (&["1.20.x"], "1.20.4", Yes),
            (&["1.20.x"], "1.21", No),
            (&["1.20.*"], "1.20", Yes),
            (&["1.x"], "1.21.1", Yes),
            (&["1.x"], "2.0", No),
            (&[">=1.20 <1.21"], "1.20.6", Yes),
            (&[">=1.20 <1.21"], "1.21", No),
            (&[">=1.20 <1.21"], "1.19.4", No),
            (&[">1.20"], "1.20", No),
            (&[">1.20"], "1.20.1", Yes),
            (&["<=1.20.1"], "1.20.1", Yes),
            (&["<1.20.1"], "1.20.1", No),
            (&["~1.20.1"], "1.20.4", Yes),
            (&["~1.20.1"], "1.20", No),
            (&["~1.20.1"], "1.21", No),
            (&["^1.20"], "1.21.4", Yes),
            (&["^1.20"], "2.0", No),
            (&["1.20.1"], "1.20.1", Yes),
            (&["1.20.1"], "1.20.2", No),
            (&["=1.20.1"], "1.20.1", Yes),
            (&["1.19.x", "1.20.x"], "1.20.1", Yes),
            (&["1.19.x", "1.20.x"], "1.18", No),
            (&["*"], "24w14a", Unknown),
        ];
        for (ranges, server, want) in table {
            assert_eq!(fabric(ranges, server), *want, "{ranges:?} contre {server}");
        }
    }

    #[test]
    fn maven_ranges_in_a_table() {
        use Containment::*;
        let table = [
            ("[1.20.1,1.21)", "1.20.1", Yes),
            ("[1.20.1,1.21)", "1.20.6", Yes),
            ("[1.20.1,1.21)", "1.21", No),
            ("[1.20.1,1.21)", "1.20", No),
            ("(1.20.1,1.21)", "1.20.1", No),
            ("[1.20.1]", "1.20.1", Yes),
            ("[1.20.1]", "1.20.2", No),
            ("[1.20,)", "1.21.4", Yes),
            ("(,1.21]", "1.21", Yes),
            ("(,1.21]", "1.21.1", No),
            ("1.20.1", "1.20.4", Yes),
            ("1.20.1", "1.19", No),
            ("[1.19,1.20),[1.20.2,1.21)", "1.20.2", Yes),
            ("[1.19,1.20),[1.20.2,1.21)", "1.20.1", No),
            ("[26.3,)", "26.3", Yes),
        ];
        for (range, server, want) in table {
            assert_eq!(maven(range, server), want, "{range} contre {server}");
        }
    }

    #[test]
    fn an_unknown_server_version_or_a_range_not_understood_is_never_a_refusal() {
        let req = VersionReq::parse_maven("[1.20.1,1.21)").unwrap();
        assert_eq!(req.contains(None), Containment::Unknown);
        for bad in [
            "",
            "[",
            "[1.20",
            "[1.20,1.21,1.22)",
            "(1.20]x",
            "[24w14a,)",
            "[1.20.1,1.21)x",
            "[1.20,1.21),",
            "(1.20)",
            "{1.20}",
        ] {
            assert!(VersionReq::parse_maven(bad).is_none(), "{bad:?}");
        }
        for bad in [
            "", "   ", ">=", "~", "^x", "1.20.x.x", "snapshot", ">=24w14a",
        ] {
            assert!(VersionReq::parse_fabric(&[bad]).is_none(), "{bad:?}");
        }
        assert!(VersionReq::parse_fabric(&[]).is_none());
        // Aucune panique sur un texte hostile.
        for hostile in ["\u{202e}1.20", "é[1,2)", "[é,2)", "[1,é)", "1.é"] {
            let _ = VersionReq::parse_maven(hostile);
            let _ = VersionReq::parse_fabric(&[hostile]);
        }
        assert!(VersionReq::parse_fabric(&[&"1".repeat(300)]).is_none());
    }
}
