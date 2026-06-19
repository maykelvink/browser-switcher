use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

pub type ModelResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UrlSpecificPreferences {
    pub firefox_profile_name: String,
    #[serde(rename = "urls")]
    pub url_regexes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FirefoxProfilePreferences {
    pub preferences: Vec<UrlSpecificPreferences>,
}

impl FirefoxProfilePreferences {
    pub fn load_from_file(path: impl AsRef<Path>) -> ModelResult<Self> {
        let content = fs::read_to_string(path)?;
        let preferences: Self = serde_json::from_str(&content)?;
        preferences.validate()?;

        Ok(preferences)
    }

    pub fn validate(&self) -> ModelResult<()> {
        for preference in &self.preferences {
            preference.validate()?;
        }

        Ok(())
    }
}

impl UrlSpecificPreferences {
    pub fn matches_url(&self, url: &str) -> ModelResult<bool> {
        for url_regex in &self.url_regexes {
            if Regex::new(url_regex)?.is_match(url) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn validate(&self) -> ModelResult<()> {
        for url_regex in &self.url_regexes {
            Regex::new(url_regex).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Invalid URL regex '{url_regex}': {error}"),
                )
            })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn loads_preferences_from_json() {
        let path = temp_model_file();
        let expected = FirefoxProfilePreferences {
            preferences: vec![UrlSpecificPreferences {
                firefox_profile_name: "work".to_string(),
                url_regexes: vec![r"^https://forgejo\.org(/.*)?$".to_string()],
            }],
        };

        fs::write(
            &path,
            serde_json::to_string(&expected).expect("json should serialize"),
        )
        .expect("preferences file should be written");

        let actual =
            FirefoxProfilePreferences::load_from_file(&path).expect("preferences should load");

        assert_eq!(actual, expected);

        fs::remove_file(path).expect("temporary model file should be removed");
    }

    #[test]
    fn reports_invalid_url_regexes() {
        let preferences = FirefoxProfilePreferences {
            preferences: vec![UrlSpecificPreferences {
                firefox_profile_name: "broken".to_string(),
                url_regexes: vec!["[".to_string()],
            }],
        };

        let error = preferences
            .validate()
            .expect_err("invalid regex should fail validation");

        assert!(error.to_string().contains("Invalid URL regex '['"));
    }

    #[test]
    fn matches_urls_using_regexes() {
        let preferences = UrlSpecificPreferences {
            firefox_profile_name: "work".to_string(),
            url_regexes: vec![r"^https://forgejo\.org(/.*)?$".to_string()],
        };

        assert!(
            preferences
                .matches_url("https://forgejo.org/docs")
                .expect("regex should compile")
        );
        assert!(
            !preferences
                .matches_url("https://example.com")
                .expect("regex should compile")
        );
    }

    fn temp_model_file() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();

        env::temp_dir().join(format!("browser-switcher-model-{nanos}.json"))
    }
}
