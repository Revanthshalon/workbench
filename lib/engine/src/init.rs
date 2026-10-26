use std::path::PathBuf;

use crate::{ConfigStore, Error, Platform};

/// Creates the configuration in `store`.
///
/// Returns where it was written and the platforms to set up: `platforms`
/// without duplicates, or the defaults if empty.
pub fn init(
    store: &impl ConfigStore,
    platforms: Vec<Platform>,
) -> Result<(PathBuf, Vec<Platform>), Error> {
    let mut unique = Vec::new();
    for p in platforms {
        if !unique.contains(&p) {
            unique.push(p);
        }
    }
    if unique.is_empty() {
        unique = Platform::DEFAULTS.to_vec();
    }
    Ok((store.create()?, unique))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    impl ConfigStore for Fake {
        fn create(&self) -> Result<PathBuf, Error> {
            Ok(PathBuf::from("fake"))
        }
    }

    #[test]
    fn dedupes_and_defaults() -> Result<(), Error> {
        let (_, p) = init(&Fake, vec![Platform::Codex, Platform::Codex])?;
        assert_eq!(p, [Platform::Codex]);
        let (_, p) = init(&Fake, vec![])?;
        assert_eq!(p, Platform::DEFAULTS);
        Ok(())
    }
}
