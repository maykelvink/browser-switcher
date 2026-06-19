#[path = "Model.rs"]
mod model;

use directories::ProjectDirs;
use model::FirefoxProfilePreferences;
use rusqlite::{Connection, OptionalExtension};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct CliInput {
    preferences_path: PathBuf,
    url: String,
}

#[derive(Debug)]
struct FirefoxProfile {
    path: String,
    name: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = parse_args(env::args().skip(1).collect())?;

    let firefox_preferences = FirefoxProfilePreferences::load_from_file(&input.preferences_path)
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Could not load preferences from {}: {error}",
                    input.preferences_path.display()
                ),
            )
        })?;
    let mut matched_preferences = None;

    for preferences in &firefox_preferences.preferences {
        if preferences.matches_url(&input.url)? {
            matched_preferences = Some(preferences);
            break;
        }
    }

    let matched_preferences = matched_preferences
        .ok_or_else(|| format!("No Firefox profile preference matched URL: {}", input.url))?;

    let profile_name = open_firefox_profile(&matched_preferences.firefox_profile_name, &input.url)?;

    println!("Started Firefox profile: {profile_name}");

    Ok(())
}

fn parse_args(args: Vec<String>) -> Result<CliInput, io::Error> {
    match args.as_slice() {
        [url] => Ok(CliInput {
            preferences_path: default_preferences_path()?,
            url: url.clone(),
        }),
        [flag, preferences_path, url] if flag == "--config" || flag == "-c" => Ok(CliInput {
            preferences_path: PathBuf::from(preferences_path),
            url: url.clone(),
        }),
        [preferences_path, url] => Ok(CliInput {
            preferences_path: PathBuf::from(preferences_path),
            url: url.clone(),
        }),
        _ => Err(usage_error()),
    }
}

fn default_preferences_path() -> Result<PathBuf, io::Error> {
    let project_dirs = ProjectDirs::from("", "", "browser-switcher").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not determine the platform config directory",
        )
    })?;

    Ok(project_dirs.config_dir().join("preferences.json"))
}

fn usage_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "Usage: browser-switcher <url>\n       browser-switcher --config <preferences.json> <url>",
    )
}

fn open_firefox_profile(
    profile_name: &str,
    url: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let home = env::var("HOME")?;
    let firefox_dir = PathBuf::from(home).join("snap/firefox/common/.mozilla/firefox");

    let store_id = find_store_id(&firefox_dir.join("profiles.ini"))?;
    let db_path = firefox_dir
        .join("Profile Groups")
        .join(format!("{store_id}.sqlite"));

    let profile = find_profile_by_name(&db_path, profile_name)?
        .ok_or_else(|| format!("No Firefox profile found with name: {profile_name}"))?;

    let profile_path = firefox_dir.join(&profile.path);
    let mut command = Command::new("/snap/bin/firefox");

    if is_profile_running(&profile_path) {
        command
            .arg("--profile")
            .arg(&profile_path)
            .arg("--new-tab")
            .arg(url);
    } else {
        command
            .arg("--new-instance")
            .arg("--no-remote")
            .arg("--profile")
            .arg(&profile_path)
            .arg(url);
    }

    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(profile.name)
}

fn is_profile_running(profile_path: &Path) -> bool {
    profile_path.join(".parentlock").exists() || profile_path.join("lock").exists()
}

fn find_store_id(profiles_ini: &Path) -> io::Result<String> {
    let content = fs::read_to_string(profiles_ini)?;

    content
        .lines()
        .find_map(|line| {
            line.strip_prefix("StoreID=")
                .map(|value| value.trim().to_string())
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No StoreID found in profiles.ini"))
}

fn find_profile_by_name(
    db_path: &Path,
    profile_name: &str,
) -> rusqlite::Result<Option<FirefoxProfile>> {
    let conn = Connection::open(db_path)?;

    conn.query_row(
        r#"
        SELECT path, name
        FROM Profiles
        WHERE lower(name) = lower(?1)
        "#,
        [profile_name],
        |row| {
            Ok(FirefoxProfile {
                path: row.get("path")?,
                name: row.get("name")?,
            })
        },
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_url_with_default_preferences_path() {
        let input = parse_args(vec!["https://example.com".to_string()]).expect("args should parse");

        assert_eq!(input.url, "https://example.com");
        assert!(input.preferences_path.ends_with("preferences.json"));
    }

    #[test]
    fn parses_explicit_preferences_path() {
        let input = parse_args(vec![
            "--config".to_string(),
            "custom.json".to_string(),
            "https://example.com".to_string(),
        ])
        .expect("args should parse");

        assert_eq!(input.preferences_path, PathBuf::from("custom.json"));
        assert_eq!(input.url, "https://example.com");
    }

    #[test]
    fn still_parses_legacy_positional_preferences_path() {
        let input = parse_args(vec![
            "custom.json".to_string(),
            "https://example.com".to_string(),
        ])
        .expect("args should parse");

        assert_eq!(input.preferences_path, PathBuf::from("custom.json"));
        assert_eq!(input.url, "https://example.com");
    }
}
