//! Version de l'agent, telle qu'affichée par `hearth-agent --version` : trois nombres.

use std::fmt;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("version illisible : {0:?}")]
pub struct VersionError(pub String);

impl Version {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Lit `X.Y.Z` (un suffixe de pré-version ou de construction après `-` ou `+` est ignoré).
    pub fn parse(text: &str) -> Result<Self, VersionError> {
        let bad = || VersionError(text.to_owned());
        let core = text
            .trim()
            .split(['-', '+'])
            .next()
            .ok_or_else(bad)?
            .trim_start_matches('v');
        let mut parts = core.split('.');
        let mut next = || -> Result<u32, VersionError> {
            parts
                .next()
                .filter(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
                .ok_or_else(bad)?
                .parse()
                .map_err(|_| bad())
        };
        let version = Self::new(next()?, next()?, next()?);
        if parts.next().is_some() {
            return Err(bad());
        }
        Ok(version)
    }

    /// Lit la sortie de `hearth-agent --version` (`hearth-agent 0.1.0`).
    pub fn parse_version_output(output: &str) -> Result<Self, VersionError> {
        let word = output
            .split_whitespace()
            .find(|word| word.starts_with(|c: char| c.is_ascii_digit()))
            .ok_or_else(|| VersionError(output.trim().to_owned()))?;
        Self::parse(word)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_numbers_are_a_version_and_they_compare_numerically() {
        assert_eq!(Version::parse("0.1.0"), Ok(Version::new(0, 1, 0)));
        assert!(Version::parse("0.10.0").unwrap() > Version::parse("0.9.9").unwrap());
        assert!(Version::parse("1.0.0").unwrap() > Version::parse("0.99.99").unwrap());
        assert_eq!(Version::new(1, 2, 3).to_string(), "1.2.3");
    }

    #[test]
    fn a_pre_release_suffix_and_a_v_prefix_are_ignored() {
        assert_eq!(Version::parse("v1.2.3-rc.1+abc"), Ok(Version::new(1, 2, 3)));
    }

    #[test]
    fn anything_else_is_unreadable() {
        for text in ["", "1.2", "1.2.3.4", "a.b.c", "1..3", "-1.2.3", "1.2.x"] {
            assert!(Version::parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn the_version_flag_output_is_read() {
        assert_eq!(
            Version::parse_version_output("hearth-agent 0.1.0\n"),
            Ok(Version::new(0, 1, 0))
        );
        assert!(Version::parse_version_output("hearth-agent\n").is_err());
        assert!(Version::parse_version_output("").is_err());
    }
}
