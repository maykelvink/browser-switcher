#[path = "Model.rs"]
mod model;

use directories::ProjectDirs;
use model::{BrowserIds, FirefoxProfilePreferences, save_browser_ids_to_file};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct CliInput {
    preferences_path: PathBuf,
    url: String,
}

struct FirefoxProfilesIni {
    store_id: String,
}

#[derive(Debug)]
struct FirefoxProfile {
    id: i64,
    path: String,
    name: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = parse_args(env::args().skip(1).collect())?;
    ensure_browser_ids_file()?;

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
    let profile_name = select_firefox_profile_name(&firefox_preferences, &input.url)?;
    let profile_name = open_firefox_profile(profile_name, &input.url)?;

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
    Ok(default_config_dir()?.join("preferences.json"))
}

fn default_browser_ids_path() -> Result<PathBuf, io::Error> {
    Ok(default_config_dir()?.join("browsers.json"))
}

fn default_config_dir() -> Result<PathBuf, io::Error> {
    let project_dirs = ProjectDirs::from("", "", "browser-switcher").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not determine the platform config directory",
        )
    })?;

    Ok(project_dirs.config_dir().to_path_buf())
}

fn usage_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "Usage: browser-switcher <url>\n       browser-switcher --config <preferences.json> <url>",
    )
}

fn select_firefox_profile_name<'a>(
    firefox_preferences: &'a FirefoxProfilePreferences,
    url: &str,
) -> Result<&'a str, Box<dyn std::error::Error>> {
    for preferences in &firefox_preferences.preferences {
        if preferences.matches_url(url)? {
            return Ok(&preferences.firefox_profile_name);
        }
    }

    firefox_preferences
        .default_firefox_profile_name
        .as_deref()
        .ok_or_else(|| format!("No Firefox profile preference matched URL: {url}").into())
}

fn open_firefox_profile(
    profile_name: &str,
    url: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let firefox_dir = default_firefox_dir()?;

    let profiles_ini = read_firefox_profiles_ini(&firefox_dir.join("profiles.ini"))?;
    let db_path = firefox_dir
        .join("Profile Groups")
        .join(format!("{}.sqlite", profiles_ini.store_id));

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

fn ensure_browser_ids_file() -> Result<(), Box<dyn std::error::Error>> {
    let browser_ids_path = default_browser_ids_path()?;

    if file_has_content(&browser_ids_path)? {
        return Ok(());
    }

    let browser_ids = load_browser_ids_from_firefox()?;
    save_browser_ids_to_file(browser_ids_path, &browser_ids)?;

    Ok(())
}

fn file_has_content(path: &Path) -> io::Result<bool> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(!content.trim().is_empty()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn load_browser_ids_from_firefox() -> Result<Vec<BrowserIds>, Box<dyn std::error::Error>> {
    let firefox_dir = default_firefox_dir()?;
    load_browser_ids_from_firefox_dir(&firefox_dir)
}

fn load_browser_ids_from_firefox_dir(
    firefox_dir: &Path,
) -> Result<Vec<BrowserIds>, Box<dyn std::error::Error>> {
    let profiles_ini = read_firefox_profiles_ini(&firefox_dir.join("profiles.ini"))?;
    let db_path = firefox_dir
        .join("Profile Groups")
        .join(format!("{}.sqlite", profiles_ini.store_id));

    Ok(load_browser_ids_from_profile_database(&db_path)?)
}

fn default_firefox_dir() -> Result<PathBuf, env::VarError> {
    let home = env::var("HOME")?;

    Ok(PathBuf::from(home).join("snap/firefox/common/.mozilla/firefox"))
}

fn is_profile_running(profile_path: &Path) -> bool {
    profile_path.join(".parentlock").exists() || profile_path.join("lock").exists()
}

fn read_firefox_profiles_ini(profiles_ini: &Path) -> io::Result<FirefoxProfilesIni> {
    let content = fs::read_to_string(profiles_ini)?;
    let mut store_id = None;

    for line in content.lines() {
        if let Some(value) = line.strip_prefix("StoreID=") {
            store_id = Some(value.trim().to_string());
        }
    }

    let store_id = store_id.ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "No StoreID found in profiles.ini")
    })?;

    Ok(FirefoxProfilesIni { store_id })
}

fn find_profile_by_name(
    db_path: &Path,
    profile_name: &str,
) -> rusqlite::Result<Option<FirefoxProfile>> {
    let conn = open_profile_database_read_only(db_path)?;

    conn.query_row(
        r#"
        SELECT id, path, name
        FROM Profiles
        WHERE lower(name) = lower(?1)
        "#,
        [profile_name],
        |row| {
            Ok(FirefoxProfile {
                id: row.get("id")?,
                path: row.get("path")?,
                name: row.get("name")?,
            })
        },
    )
    .optional()
}

fn load_browser_ids_from_profile_database(db_path: &Path) -> rusqlite::Result<Vec<BrowserIds>> {
    let conn = open_profile_database_read_only(db_path)?;
    let mut statement = conn.prepare(
        r#"
        SELECT id, path, name
        FROM Profiles
        ORDER BY id
        "#,
    )?;

    let profiles = statement.query_map([], |row| {
        Ok(FirefoxProfile {
            id: row.get("id")?,
            path: row.get("path")?,
            name: row.get("name")?,
        })
    })?;

    let mut browser_ids = Vec::new();

    for profile in profiles {
        let profile = profile?;

        browser_ids.push(BrowserIds {
            browser_id: profile.id as u64,
            firefox_profile_path: profile.path,
            firefox_profile_name: profile.name,
        });
    }

    Ok(browser_ids)
}

fn open_profile_database_read_only(db_path: &Path) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
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

    #[test]
    fn selects_matching_profile_before_default() {
        let preferences = FirefoxProfilePreferences {
            default_firefox_profile_name: Some("Default".to_string()),
            preferences: vec![model::UrlSpecificPreferences {
                firefox_profile_name: "Work".to_string(),
                url_regexes: vec![r"^https://forgejo\.org(/.*)?$".to_string()],
            }],
        };

        let profile_name = select_firefox_profile_name(&preferences, "https://forgejo.org/docs")
            .expect("profile should resolve");

        assert_eq!(profile_name, "Work");
    }

    #[test]
    fn selects_default_profile_when_no_url_matches() {
        let preferences = FirefoxProfilePreferences {
            default_firefox_profile_name: Some("Default".to_string()),
            preferences: vec![model::UrlSpecificPreferences {
                firefox_profile_name: "Work".to_string(),
                url_regexes: vec![r"^https://forgejo\.org(/.*)?$".to_string()],
            }],
        };

        let profile_name = select_firefox_profile_name(&preferences, "https://example.com")
            .expect("default profile should resolve");

        assert_eq!(profile_name, "Default");
    }

    #[test]
    fn detects_empty_and_missing_files_as_no_content() {
        let path = env::temp_dir().join(format!(
            "browser-switcher-empty-{}.json",
            std::process::id()
        ));

        assert!(!file_has_content(&path).expect("missing file should be readable as empty"));

        fs::write(&path, "").expect("empty file should be written");
        assert!(!file_has_content(&path).expect("empty file should be readable"));

        fs::write(&path, "[]").expect("non-empty file should be written");
        assert!(file_has_content(&path).expect("non-empty file should be readable"));

        fs::remove_file(path).expect("temporary file should be removed");
    }

    #[test]
    fn parses_firefox_profiles_ini() {
        let path = env::temp_dir().join(format!(
            "browser-switcher-profiles-{}.ini",
            std::process::id()
        ));
        fs::write(
            &path,
            r#"
[Profile0]
Name=default
Path=abc.default
StoreID=8fef57bf

[Profile1]
Name=work
Path=def.work
"#,
        )
        .expect("profiles.ini should be written");

        let profiles_ini = read_firefox_profiles_ini(&path).expect("profiles.ini should parse");

        assert_eq!(profiles_ini.store_id, "8fef57bf");
        fs::remove_file(path).expect("temporary file should be removed");
    }

    #[test]
    fn loads_browser_ids_from_profile_database() {
        let path = env::temp_dir().join(format!(
            "browser-switcher-profiles-{}.sqlite",
            std::process::id()
        ));

        {
            let conn = Connection::open(&path).expect("database should open");
            conn.execute(
                r#"
                CREATE TABLE Profiles (
                    id INTEGER NOT NULL PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    name TEXT NOT NULL
                )
                "#,
                [],
            )
            .expect("profiles table should be created");
            conn.execute(
                "INSERT INTO Profiles (id, path, name) VALUES (?1, ?2, ?3)",
                (1, "abc.default", "Default"),
            )
            .expect("first profile should be inserted");
            conn.execute(
                "INSERT INTO Profiles (id, path, name) VALUES (?1, ?2, ?3)",
                (2, "def.work", "Work"),
            )
            .expect("second profile should be inserted");
        }

        let browser_ids =
            load_browser_ids_from_profile_database(&path).expect("browser ids should load");

        assert_eq!(
            browser_ids,
            vec![
                BrowserIds {
                    browser_id: 1,
                    firefox_profile_path: "abc.default".to_string(),
                    firefox_profile_name: "Default".to_string(),
                },
                BrowserIds {
                    browser_id: 2,
                    firefox_profile_path: "def.work".to_string(),
                    firefox_profile_name: "Work".to_string(),
                },
            ]
        );

        fs::remove_file(path).expect("temporary database should be removed");
    }
}
