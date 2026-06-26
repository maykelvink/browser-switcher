# Browser Switcher

Small Linux browser router for Firefox profiles. It reads a URL, matches it against regex rules in a JSON config file, and opens the URL in the configured Firefox profile. If no rule matches, it can fall back to a default Firefox profile.

The app is currently built for Firefox installed through Snap:

```text
~/snap/firefox/common/.mozilla/firefox
```

## Config Files

Runtime config lives in the XDG config directory:

```text
~/.config/browser-switcher/
```

The main routing file is:

```text
~/.config/browser-switcher/preferences.json
```

Example:

```json
{
  "default_firefox_profile_name": "Personal",
  "preferences": [
    {
      "firefox_profile_name": "Local",
      "urls": [".*.example\\.nl.*"]
    }
  ]
}
```

The `urls` values are Rust regex expressions. The first matching rule wins. If no rule matches, `default_firefox_profile_name` is used.

The app also creates this file on first run if it is missing or empty:

```text
~/.config/browser-switcher/browsers.json
```

That file is generated from Firefox's profile-group SQLite database and contains all known Firefox profiles:

```json
[
  {
    "browserId": 1,
    "firefox_profile_path": "bwpjwmfl.personal",
    "firefox_profile_name": "Original profile"
  }
]
```

Firefox's SQLite database is opened read-only.

## Build

```bash
cargo build --release
```

## Install

Install the binary somewhere stable:

```bash
install -D -m 755 target/release/browser-switcher ~/.local/bin/browser-switcher
```

Create the desktop entry:

```bash
mkdir -p ~/.local/share/applications
cat > ~/.local/share/applications/browser-switcher.desktop <<'EOF'
[Desktop Entry]
Version=1.0
Type=Application
Name=Browser Switcher
Comment=Route URLs to Firefox profiles
Exec=/home/maykel/.local/bin/browser-switcher %u
Terminal=false
NoDisplay=true
Categories=Network;WebBrowser;
MimeType=text/html;text/xml;application/xhtml+xml;x-scheme-handler/http;x-scheme-handler/https;
EOF
```

Refresh the desktop database:

```bash
update-desktop-database ~/.local/share/applications
```

Set it as the default browser:

```bash
xdg-mime default browser-switcher.desktop x-scheme-handler/http
xdg-mime default browser-switcher.desktop x-scheme-handler/https
xdg-mime default browser-switcher.desktop text/html
xdg-settings set default-web-browser browser-switcher.desktop
```

Verify:

```bash
xdg-mime query default x-scheme-handler/http
xdg-mime query default x-scheme-handler/https
xdg-settings get default-web-browser
```

Expected output:

```text
browser-switcher.desktop
browser-switcher.desktop
browser-switcher.desktop
```

## Usage

Once installed as the default browser, opening links from other apps should call Browser Switcher automatically.

Manual use:

```bash
browser-switcher https://example.com
```

Use a custom config file:

```bash
browser-switcher --config preferences.json https://example.com
```

## Development Checks

```bash
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release --all-features
```

After code changes, rebuild and reinstall:

```bash
cargo build --release
install -D -m 755 target/release/browser-switcher ~/.local/bin/browser-switcher
```
