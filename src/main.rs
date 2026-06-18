use rusqlite::{Connection, OptionalExtension};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug)]
struct FirefoxProfile {
    path: String,
    name: String,
}

fn is_profile_running(profile_path: &Path) -> bool {
    profile_path.join(".parentlock").exists() || profile_path.join("lock").exists()
}

fn find_store_id(profiles_ini: &Path) -> std::io::Result<String> {
    let content = fs::read_to_string(profiles_ini)?;

    content
        .lines()
        .find_map(|line| line.strip_prefix("StoreID=").map(|v| v.trim().to_string()))
        .ok_or_else(|| std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No StoreID found in profiles.ini",
        ))
}

fn find_profile_by_name(db_path: &Path, profile_name: &str) -> rusqlite::Result<Option<FirefoxProfile>> {
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();

    if args.is_empty() {
        eprintln!("Usage: ff-router <profile-name> [url]");
        eprintln!("Example: ff-router secure https://chatgpt.com");
        std::process::exit(1);
    }

    let profile_name = args.remove(0);

    let home = env::var("HOME")?;
    let firefox_dir = PathBuf::from(home)
        .join("snap/firefox/common/.mozilla/firefox");

    let store_id = find_store_id(&firefox_dir.join("profiles.ini"))?;

    let db_path = firefox_dir
        .join("Profile Groups")
        .join(format!("{store_id}.sqlite"));

    let profile = find_profile_by_name(&db_path, &profile_name)?
        .ok_or_else(|| format!("No Firefox profile found with name: {profile_name}"))?;

    let profile_path = firefox_dir.join(&profile.path);

    let mut command = Command::new("/snap/bin/firefox");

    if is_profile_running(&profile_path) {
        // Reuse already-running profile
        command
            .arg("--profile")
            .arg(&profile_path);

        if args.is_empty() {
            command.arg("--browser");
        } else {
            for url in args {
                command.arg("--new-tab");
                command.arg(url);
            }
        }
    } else {
        // Start profile fresh
        command
            .arg("--new-instance")
            .arg("--no-remote")
            .arg("--profile")
            .arg(&profile_path);

        if args.is_empty() {
            command.arg("--browser");
        } else {
            for url in args {
                command.arg(url);
            }
        }
    }


    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;


    println!("Started Firefox profile: {}", profile.name);

    Ok(())
}