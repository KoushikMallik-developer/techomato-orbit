//! One-click databases: a pinned catalog of engines (downloaded once, checked against a fixed
//! SHA-256, unpacked under Orbit's data folder) and instances created from them. An instance is
//! an ordinary service, so starting, stopping, logs and health work as for any other.

pub mod catalog;
pub mod install;
pub mod instance;

/// First free port at or after `default` (at most 100 tried).
pub fn pick_port(default: u16, taken: &dyn Fn(u16) -> bool) -> Option<u16> {
    (default..=default.saturating_add(100)).find(|p| !taken(*p))
}

/// A folder-safe name: lowercase letters, digits and dashes.
pub fn slug(name: &str) -> String {
    let s: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "db".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_free_port_at_or_after_the_default_is_chosen() {
        assert_eq!(pick_port(5432, &|p| p == 5432 || p == 5433), Some(5434));
        assert_eq!(pick_port(6379, &|_| false), Some(6379));
        assert_eq!(pick_port(6379, &|_| true), None);
    }

    #[test]
    fn names_become_safe_folder_names() {
        assert_eq!(slug("My DB 1"), "my-db-1");
        assert_eq!(slug(r"..\evil"), "evil");
        assert_eq!(slug("   "), "db");
    }
}
