//! One charset/collation authority for configuration and protocol metadata.

mod catalog;
pub mod codec;
mod lexer_space;
pub mod multibyte;
mod multibyte_delta;
mod single_byte;

/// MySQL-specific multibyte families. Similar Web encodings are not aliases;
/// each route applies its own verified validity and mapping differences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Multibyte {
    /// Traditional Chinese Big5.
    Big5,
    /// Microsoft Japanese CP932.
    Cp932,
    /// Microsoft Japanese EUC extension.
    Eucjpms,
    /// MySQL Korean EUC repertoire.
    Euckr,
    /// MySQL GB18030, including its four-byte ranges.
    Gb18030,
    /// GB2312's restricted repertoire.
    Gb2312,
    /// MySQL GBK.
    Gbk,
    /// MySQL's Shift JIS repertoire.
    Sjis,
    /// Japanese EUC/JIS X 0212.
    Ujis,
}

/// Static route selected once, before encoding SQL or preparing column output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecRoute {
    /// Native UTF-8.
    Utf8,
    /// Strict native ASCII.
    Ascii,
    /// MySQL's byte-valued pseudo charset.
    Binary,
    /// A specific verified MySQL single-byte table.
    SingleByte(u8),
    /// A specific MySQL multibyte family.
    Multibyte(Multibyte),
    /// Known metadata family excluded from client charset selection.
    Unavailable,
}

/// Canonical server catalog identity and its actual encoding route.
#[derive(Debug)]
pub struct Charset {
    /// Canonical MySQL character-set name.
    pub name: &'static str,
    /// MySQL's default collation when only this charset is selected.
    pub default_collation: &'static str,
    /// Maximum encoded width reported by the real server catalog.
    pub max_width: u8,
    /// The unique codec implementation family.
    pub route: CodecRoute,
}

/// A real protocol collation identifier, not an array index or truncated byte.
#[derive(Debug)]
pub struct Collation {
    /// Canonical MySQL collation name.
    pub name: &'static str,
    /// Canonical associated charset name.
    pub charset: &'static str,
    /// Complete MySQL identifier used by column metadata and session settings.
    pub id: u16,
}

/// One validated connection-encoding selection, borrowing immutable catalogs.
#[derive(Debug, Clone, Copy)]
pub struct Resolved {
    /// Actual charset and route.
    pub charset: &'static Charset,
    /// Actual compatible collation.
    pub collation: &'static Collation,
}

/// Invalid configuration tokens or combinations, before network operations.
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    /// The charset was not in the supported canonical/explicit alias set.
    #[error("unknown or unsupported client charset")]
    Charset,
    /// The collation was unknown or belonged to an excluded client charset.
    #[error("unknown or unsupported client collation")]
    Collation,
    /// The two supplied catalog identities do not belong together.
    #[error("charset and collation do not match")]
    Mismatch,
}

/// Resolve missing values, approved UTF-8 aliases and strict catalog pairing.
///
/// # Errors
/// Returns unknown/excluded-token or mismatched-pair failures. Empty strings
/// are invalid; they do not mean the same thing as an omitted value.
pub fn resolve(charset: Option<&str>, collation: Option<&str>) -> Result<Resolved, CatalogError> {
    let charset = charset
        .map(|name| {
            let name = name.trim_matches(|c: char| c.is_ascii_whitespace());
            let name = if name.eq_ignore_ascii_case("utf8") || name.eq_ignore_ascii_case("utf-8") {
                "utf8mb4"
            } else {
                name
            };
            catalog::CHARSETS
                .iter()
                .find(|item| {
                    item.name.eq_ignore_ascii_case(name) && item.route != CodecRoute::Unavailable
                })
                .ok_or(CatalogError::Charset)
        })
        .transpose()?;
    let collation = collation
        .map(|name| {
            let name = name.trim_matches(|c: char| c.is_ascii_whitespace());
            catalog::COLLATIONS
                .iter()
                .find(|item| item.name.eq_ignore_ascii_case(name))
                .ok_or(CatalogError::Collation)
        })
        .transpose()?;
    let charset = match (charset, collation) {
        (Some(charset), _) => charset,
        (None, Some(collation)) => catalog::CHARSETS
            .iter()
            .find(|charset| {
                charset.name == collation.charset && charset.route != CodecRoute::Unavailable
            })
            .ok_or(CatalogError::Collation)?,
        (None, None) => catalog::CHARSETS
            .iter()
            .find(|charset| charset.name == "utf8mb4")
            .ok_or(CatalogError::Charset)?,
    };
    let collation = match collation {
        Some(collation) if collation.charset == charset.name => collation,
        Some(_) => return Err(CatalogError::Mismatch),
        None => catalog::COLLATIONS
            .iter()
            .find(|collation| collation.name == charset.default_collation)
            .ok_or(CatalogError::Collation)?,
    };
    Ok(Resolved { charset, collation })
}

/// Find complete column metadata by a real protocol ID. Server-only families
/// remain identifiable without making them valid client encoding selections.
#[must_use]
pub fn by_id(id: u16) -> Option<(&'static Collation, &'static Charset)> {
    let collation = catalog::COLLATIONS
        .iter()
        .find(|collation| collation.id == id)?;
    let charset = catalog::CHARSETS
        .iter()
        .find(|charset| charset.name == collation.charset)?;
    Some((collation, charset))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_defaults_and_single_parameter_resolution_match_the_real_catalog() {
        assert_eq!(resolve(None, None).unwrap().collation.id, 255);
        assert_eq!(
            resolve(Some(" UTF-8 "), None).unwrap().charset.name,
            "utf8mb4"
        );
        assert_eq!(
            resolve(None, Some("LATIN1_BIN")).unwrap().charset.name,
            "latin1"
        );
        assert_eq!(
            resolve(Some("latin1"), None).unwrap().collation.name,
            "latin1_swedish_ci"
        );
        assert!(resolve(Some("utf8mb3"), None).is_err());
        assert!(resolve(Some("utf8mb4"), Some("latin1_bin")).is_err());
        assert_eq!(by_id(278).unwrap().0.name, "utf8mb4_0900_as_cs");
    }
    #[test]
    fn every_canonical_client_route_resolves_without_dropping_full_ids() {
        let selected: Vec<_> = catalog::CHARSETS
            .iter()
            .filter(|charset| charset.route != CodecRoute::Unavailable)
            .collect();
        assert_eq!(selected.len(), 36);
        for charset in selected {
            assert_eq!(
                resolve(Some(charset.name), None).unwrap().charset.name,
                charset.name
            );
        }
        for collation in catalog::COLLATIONS {
            assert_eq!(by_id(collation.id).unwrap().0.name, collation.name);
        }
    }
}
