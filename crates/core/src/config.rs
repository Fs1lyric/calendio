//! `~/.config/calendio/google.env`: the Google Desktop OAuth client, one KEY=value per
//! line. The systemd unit loads it with EnvironmentFile, and calendio-booker reads it
//! itself too, so a sign-in started from the app sees the same values.

use std::path::{Path, PathBuf};

pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"))
        .join("calendio")
}

pub fn google_env_path() -> PathBuf {
    config_dir().join("google.env")
}

/// KEY=value pairs, in the subset systemd's EnvironmentFile and shells agree on:
/// `#` comments, blank lines, an optional `export `, and matching quotes stripped.
pub fn parse_env(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (key, value) = l.strip_prefix("export ").unwrap_or(l).split_once('=')?;
            let key = key.trim();
            if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                return None;
            }
            let value = value.trim();
            let unquoted = ['"', '\'']
                .iter()
                .find_map(|q| value.strip_prefix(*q).and_then(|v| v.strip_suffix(*q)))
                .unwrap_or(value);
            Some((key.to_owned(), unquoted.to_owned()))
        })
        .collect()
}

pub fn read_env(path: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(path).map(|t| parse_env(&t)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_the_shared_env_subset() {
        let text = "# Google Desktop client\n\nCALENDIO_GOOGLE_CLIENT_ID=123.apps.googleusercontent.com\n\
                    export CALENDIO_GOOGLE_CLIENT_SECRET=\"GOCSPX-abc=def\"\nbad key=x\nnoequals\n  SPACED = 'v' \n";
        assert_eq!(
            parse_env(text),
            [
                ("CALENDIO_GOOGLE_CLIENT_ID".to_owned(), "123.apps.googleusercontent.com".to_owned()),
                ("CALENDIO_GOOGLE_CLIENT_SECRET".to_owned(), "GOCSPX-abc=def".to_owned()),
                ("SPACED".to_owned(), "v".to_owned()),
            ]
        );
    }
    #[test]
    fn a_missing_file_is_empty() {
        assert!(read_env(Path::new("/nonexistent/calendio/google.env")).is_empty());
    }
}
