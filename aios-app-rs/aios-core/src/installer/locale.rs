//! Country-to-locale mapping for the AiOS installer.
//!
//! Provides automatic derivation of language, keyboard layout, timezone,
//! and time format from a country selection. Includes IP-based geolocation
//! for auto-detection.

use serde::Deserialize;

/// Locale defaults derived from a country selection.
#[derive(Debug, Clone)]
pub struct CountryDefaults {
    /// ISO 3166-1 alpha-2 country code (e.g. "DE").
    pub country_code: &'static str,
    /// Country display name in English (e.g. "Germany").
    pub country_name: &'static str,
    /// Language code for locale (e.g. "de").
    pub language: &'static str,
    /// Keyboard layout code (e.g. "de").
    pub keyboard: &'static str,
    /// IANA timezone (e.g. "Europe/Berlin").
    pub timezone: &'static str,
    /// Whether the country uses 24-hour time format.
    pub time_format_24h: bool,
}

/// Static table of country defaults (~40 countries).
static COUNTRIES: &[CountryDefaults] = &[
    // Americas
    CountryDefaults { country_code: "US", country_name: "United States", language: "en", keyboard: "us", timezone: "America/New_York", time_format_24h: false },
    CountryDefaults { country_code: "CA", country_name: "Canada", language: "en", keyboard: "us", timezone: "America/Toronto", time_format_24h: false },
    CountryDefaults { country_code: "MX", country_name: "Mexico", language: "es", keyboard: "latam", timezone: "America/Mexico_City", time_format_24h: true },
    CountryDefaults { country_code: "BR", country_name: "Brazil", language: "pt", keyboard: "br", timezone: "America/Sao_Paulo", time_format_24h: true },
    CountryDefaults { country_code: "AR", country_name: "Argentina", language: "es", keyboard: "latam", timezone: "America/Argentina/Buenos_Aires", time_format_24h: true },

    // Western Europe
    CountryDefaults { country_code: "GB", country_name: "United Kingdom", language: "en", keyboard: "gb", timezone: "Europe/London", time_format_24h: true },
    CountryDefaults { country_code: "IE", country_name: "Ireland", language: "en", keyboard: "gb", timezone: "Europe/Dublin", time_format_24h: true },
    CountryDefaults { country_code: "DE", country_name: "Germany", language: "de", keyboard: "de", timezone: "Europe/Berlin", time_format_24h: true },
    CountryDefaults { country_code: "FR", country_name: "France", language: "fr", keyboard: "fr", timezone: "Europe/Paris", time_format_24h: true },
    CountryDefaults { country_code: "ES", country_name: "Spain", language: "es", keyboard: "es", timezone: "Europe/Madrid", time_format_24h: true },
    CountryDefaults { country_code: "IT", country_name: "Italy", language: "it", keyboard: "it", timezone: "Europe/Rome", time_format_24h: true },
    CountryDefaults { country_code: "PT", country_name: "Portugal", language: "pt", keyboard: "pt", timezone: "Europe/Lisbon", time_format_24h: true },
    CountryDefaults { country_code: "NL", country_name: "Netherlands", language: "nl", keyboard: "us", timezone: "Europe/Amsterdam", time_format_24h: true },
    CountryDefaults { country_code: "BE", country_name: "Belgium", language: "nl", keyboard: "be", timezone: "Europe/Brussels", time_format_24h: true },
    CountryDefaults { country_code: "AT", country_name: "Austria", language: "de", keyboard: "de", timezone: "Europe/Vienna", time_format_24h: true },
    CountryDefaults { country_code: "CH", country_name: "Switzerland", language: "de", keyboard: "ch", timezone: "Europe/Zurich", time_format_24h: true },

    // Scandinavia
    CountryDefaults { country_code: "SE", country_name: "Sweden", language: "sv", keyboard: "se", timezone: "Europe/Stockholm", time_format_24h: true },
    CountryDefaults { country_code: "NO", country_name: "Norway", language: "nb", keyboard: "no", timezone: "Europe/Oslo", time_format_24h: true },
    CountryDefaults { country_code: "DK", country_name: "Denmark", language: "da", keyboard: "dk", timezone: "Europe/Copenhagen", time_format_24h: true },
    CountryDefaults { country_code: "FI", country_name: "Finland", language: "fi", keyboard: "fi", timezone: "Europe/Helsinki", time_format_24h: true },

    // Eastern Europe
    CountryDefaults { country_code: "PL", country_name: "Poland", language: "pl", keyboard: "pl", timezone: "Europe/Warsaw", time_format_24h: true },
    CountryDefaults { country_code: "CZ", country_name: "Czech Republic", language: "cs", keyboard: "cz", timezone: "Europe/Prague", time_format_24h: true },
    CountryDefaults { country_code: "RO", country_name: "Romania", language: "ro", keyboard: "ro", timezone: "Europe/Bucharest", time_format_24h: true },
    CountryDefaults { country_code: "HU", country_name: "Hungary", language: "hu", keyboard: "hu", timezone: "Europe/Budapest", time_format_24h: true },
    CountryDefaults { country_code: "GR", country_name: "Greece", language: "el", keyboard: "gr", timezone: "Europe/Athens", time_format_24h: true },
    CountryDefaults { country_code: "RU", country_name: "Russia", language: "ru", keyboard: "ru", timezone: "Europe/Moscow", time_format_24h: true },
    CountryDefaults { country_code: "TR", country_name: "Turkey", language: "tr", keyboard: "tr", timezone: "Europe/Istanbul", time_format_24h: true },

    // Middle East
    CountryDefaults { country_code: "IL", country_name: "Israel", language: "he", keyboard: "il", timezone: "Asia/Jerusalem", time_format_24h: true },
    CountryDefaults { country_code: "SA", country_name: "Saudi Arabia", language: "ar", keyboard: "ara", timezone: "Asia/Riyadh", time_format_24h: false },
    CountryDefaults { country_code: "AE", country_name: "United Arab Emirates", language: "ar", keyboard: "ara", timezone: "Asia/Dubai", time_format_24h: false },

    // Asia
    CountryDefaults { country_code: "JP", country_name: "Japan", language: "ja", keyboard: "jp", timezone: "Asia/Tokyo", time_format_24h: true },
    CountryDefaults { country_code: "CN", country_name: "China", language: "zh", keyboard: "cn", timezone: "Asia/Shanghai", time_format_24h: true },
    CountryDefaults { country_code: "KR", country_name: "South Korea", language: "ko", keyboard: "kr", timezone: "Asia/Seoul", time_format_24h: true },
    CountryDefaults { country_code: "IN", country_name: "India", language: "hi", keyboard: "in", timezone: "Asia/Kolkata", time_format_24h: true },
    CountryDefaults { country_code: "TW", country_name: "Taiwan", language: "zh", keyboard: "tw", timezone: "Asia/Taipei", time_format_24h: true },
    CountryDefaults { country_code: "SG", country_name: "Singapore", language: "en", keyboard: "us", timezone: "Asia/Singapore", time_format_24h: true },
    CountryDefaults { country_code: "MY", country_name: "Malaysia", language: "ms", keyboard: "us", timezone: "Asia/Kuala_Lumpur", time_format_24h: true },

    // Oceania
    CountryDefaults { country_code: "AU", country_name: "Australia", language: "en", keyboard: "us", timezone: "Australia/Sydney", time_format_24h: false },
    CountryDefaults { country_code: "NZ", country_name: "New Zealand", language: "en", keyboard: "us", timezone: "Pacific/Auckland", time_format_24h: false },

    // Africa
    CountryDefaults { country_code: "ZA", country_name: "South Africa", language: "en", keyboard: "za", timezone: "Africa/Johannesburg", time_format_24h: true },
];

/// Returns the full list of supported countries.
pub fn all_countries() -> &'static [CountryDefaults] {
    COUNTRIES
}

/// Look up a country by its ISO 3166-1 alpha-2 code (case-insensitive).
pub fn country_by_code(code: &str) -> Option<&'static CountryDefaults> {
    let upper = code.to_uppercase();
    COUNTRIES.iter().find(|c| c.country_code == upper)
}

/// Look up a country by name (case-insensitive substring match).
///
/// Returns the first match. For example, "germ" matches "Germany".
pub fn country_by_name(name: &str) -> Option<&'static CountryDefaults> {
    let lower = name.to_lowercase();
    COUNTRIES.iter().find(|c| c.country_name.to_lowercase().contains(&lower))
}

/// JSON response structure from ip-api.com.
#[derive(Deserialize)]
struct IpApiResponse {
    #[serde(rename = "countryCode")]
    country_code: Option<String>,
}

/// Auto-detect the user's country via IP geolocation.
///
/// Queries `http://ip-api.com/json/` and looks up the country code in our table.
/// Returns `None` if the request fails, times out (5 seconds), or the country
/// is not in our supported list.
pub async fn detect_country() -> Option<&'static CountryDefaults> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;

    let resp: IpApiResponse = client
        .get("http://ip-api.com/json/")
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;

    let code = resp.country_code?;
    country_by_code(&code)
}

/// Blocking version of [`detect_country`] — spawns a thread and invokes the
/// callback with the result. Suitable for use from the GTK main thread.
pub fn detect_country_blocking(callback: impl FnOnce(Option<&'static CountryDefaults>) + Send + 'static) {
    std::thread::spawn(move || {
        let result = detect_country_sync();
        callback(result);
    });
}

/// Synchronous country detection using `reqwest::blocking`.
///
/// Public variant for direct use from background threads.
/// Returns `None` if the request fails or the country is not in our table.
pub fn detect_country_sync_pub() -> Option<&'static CountryDefaults> {
    detect_country_sync()
}

/// Synchronous country detection using `reqwest::blocking`.
fn detect_country_sync() -> Option<&'static CountryDefaults> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;

    let resp: IpApiResponse = client
        .get("http://ip-api.com/json/")
        .send()
        .ok()?
        .json()
        .ok()?;

    let code = resp.country_code?;
    country_by_code(&code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_countries_not_empty() {
        let countries = all_countries();
        assert!(countries.len() >= 30, "Expected at least 30 countries, got {}", countries.len());
    }

    #[test]
    fn test_lookup_by_code() {
        let us = country_by_code("US").expect("US should exist");
        assert_eq!(us.country_name, "United States");
        assert_eq!(us.language, "en");
        assert_eq!(us.keyboard, "us");

        let de = country_by_code("DE").expect("DE should exist");
        assert_eq!(de.country_name, "Germany");
        assert_eq!(de.language, "de");
        assert_eq!(de.timezone, "Europe/Berlin");

        let jp = country_by_code("JP").expect("JP should exist");
        assert_eq!(jp.country_name, "Japan");
        assert_eq!(jp.language, "ja");
    }

    #[test]
    fn test_lookup_by_name() {
        let de = country_by_name("Germany").expect("Germany should exist");
        assert_eq!(de.country_code, "DE");

        let us = country_by_name("United States").expect("United States should exist");
        assert_eq!(us.country_code, "US");

        // Substring match
        let br = country_by_name("Braz").expect("Brazil should match 'Braz'");
        assert_eq!(br.country_code, "BR");
    }

    #[test]
    fn test_lookup_case_insensitive() {
        assert!(country_by_code("de").is_some());
        assert!(country_by_code("De").is_some());
        assert!(country_by_code("DE").is_some());

        assert!(country_by_name("germany").is_some());
        assert!(country_by_name("GERMANY").is_some());
    }

    #[test]
    fn test_unknown_country_returns_none() {
        assert!(country_by_code("XX").is_none());
        assert!(country_by_code("ZZ").is_none());
        assert!(country_by_name("Atlantis").is_none());
    }

    #[test]
    fn test_all_countries_have_valid_fields() {
        for country in all_countries() {
            assert_eq!(country.country_code.len(), 2, "Country code should be 2 chars: {}", country.country_code);
            assert!(!country.country_name.is_empty(), "Country name should not be empty");
            assert!(!country.language.is_empty(), "Language should not be empty for {}", country.country_code);
            assert!(!country.keyboard.is_empty(), "Keyboard should not be empty for {}", country.country_code);
            assert!(country.timezone.contains('/'), "Timezone should contain '/': {}", country.timezone);
        }
    }
}
