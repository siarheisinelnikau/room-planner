use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::LazyLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    English,
    Russian,
    French,
    German,
    Spanish,
    Italian,
}
impl Language {
    pub const ALL: [Self; 6] = [
        Self::English,
        Self::Russian,
        Self::French,
        Self::German,
        Self::Spanish,
        Self::Italian,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Russian => "Русский",
            Self::French => "Français",
            Self::German => "Deutsch",
            Self::Spanish => "Español",
            Self::Italian => "Italiano",
        }
    }
    pub fn text(self, key: &str) -> &'static str {
        let index = self as usize;
        STRINGS[index]
            .get(key)
            .or_else(|| STRINGS[0].get(key))
            .map(String::as_str)
            .unwrap_or("?")
    }
}
static STRINGS: LazyLock<[BTreeMap<String, String>; 6]> = LazyLock::new(|| {
    [
        include_str!("../locales/en.json"),
        include_str!("../locales/ru.json"),
        include_str!("../locales/fr.json"),
        include_str!("../locales/de.json"),
        include_str!("../locales/es.json"),
        include_str!("../locales/it.json"),
    ]
    .map(|text| serde_json::from_str(text).expect("Bundled translations must be valid"))
});

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_language_is_complete() {
        for strings in STRINGS.iter() {
            assert_eq!(
                STRINGS[0].keys().collect::<Vec<_>>(),
                strings.keys().collect::<Vec<_>>()
            );
            assert!(strings.values().all(|v| !v.trim().is_empty()));
        }
    }
}
