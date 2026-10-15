//! Ban matching and enforcement helpers.

use crate::names::sanitize_display_name;
use crate::state::Ban;
use std::time::{SystemTime, UNIX_EPOCH};

/// Known ban scopes (unknown scopes are stored but never match).
pub const SCOPES: &[&str] = &[
    "ip",
    "name",
    "name_regex",
    "fingerprint",
    "ticket_id",
    "turn_user",
    "auth_key",
    "resume_token",
];

/// Subject attributes checked against the ban list.
#[derive(Clone, Debug, Default)]
pub struct BanSubject<'a> {
    /// Client IP (or empty).
    pub ip: &'a str,
    /// Display name / pseudo (raw or already sanitized).
    pub name: &'a str,
    /// IWAD fingerprint hex.
    pub fingerprint: &'a str,
    /// Minted join ticket id.
    pub ticket_id: &'a str,
    /// Per-seat TURN username (if any).
    pub turn_user: &'a str,
    /// Resume token (sensitive — exact match only).
    pub resume_token: &'a str,
}

/// Why a subject was banned (for Reject / HTTP 403 detail).
#[derive(Clone, Debug)]
pub struct BanHit {
    /// Ban id.
    pub id: String,
    /// Matched scope.
    pub scope: String,
    /// Operator reason.
    pub reason: String,
}

/// Validate scope + value before persisting.
///
/// # Errors
///
/// Unknown scope, empty value, or oversized / invalid pattern.
pub fn validate_ban(scope: &str, value: &str) -> Result<(), String> {
    let scope = scope.trim().to_ascii_lowercase();
    if !SCOPES.contains(&scope.as_str()) {
        return Err(format!(
            "unknown scope {scope:?}; expected one of {}",
            SCOPES.join(", ")
        ));
    }
    let value = value.trim();
    if value.is_empty() {
        return Err("value required".into());
    }
    if value.len() > 256 {
        return Err("value too long (max 256 bytes)".into());
    }
    if scope == "name_regex" {
        if value.chars().count() > 64 {
            return Err("name_regex pattern too long (max 64 chars)".into());
        }
        // Reject nested quantifiers that would invite ReDoS if we ever grow the engine.
        if value.contains("**") || value.contains("++") {
            return Err("name_regex: invalid pattern".into());
        }
    }
    if scope == "ip" && value.contains('/') {
        // CIDR deferred — keep exact IP / prefix-free for now.
        return Err("ip bans are exact addresses (no CIDR yet)".into());
    }
    Ok(())
}

/// Normalize scope aliases from older UI / docs.
#[must_use]
pub fn normalize_scope(scope: &str) -> String {
    match scope.trim().to_ascii_lowercase().as_str() {
        "name_pattern" | "pseudo" | "pseudo_regex" => "name_regex".into(),
        "iwad" | "iwad_fingerprint" => "fingerprint".into(),
        "noise" | "noise_key" | "auth" => "auth_key".into(),
        "turn" => "turn_user".into(),
        "ticket" => "ticket_id".into(),
        other => other.into(),
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn active(ban: &Ban, now: u64) -> bool {
    ban.exp == 0 || ban.exp > now
}

/// Case-insensitive exact string match.
fn eq_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Glob match: `*` any run, `?` one char; case-insensitive on ASCII.
fn glob_ci(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    glob_rec(&pat, 0, &txt, 0)
}

fn glob_rec(pat: &[char], pi: usize, txt: &[char], ti: usize) -> bool {
    if pi == pat.len() {
        return ti == txt.len();
    }
    match pat[pi] {
        '*' => {
            // Greedy-safe: advance text or consume star.
            if glob_rec(pat, pi + 1, txt, ti) {
                return true;
            }
            if ti < txt.len() {
                return glob_rec(pat, pi, txt, ti + 1);
            }
            false
        }
        '?' => {
            if ti >= txt.len() {
                return false;
            }
            glob_rec(pat, pi + 1, txt, ti + 1)
        }
        c => {
            if ti >= txt.len() {
                return false;
            }
            let a = c.to_ascii_lowercase();
            let b = txt[ti].to_ascii_lowercase();
            if a != b {
                return false;
            }
            glob_rec(pat, pi + 1, txt, ti + 1)
        }
    }
}

fn match_scope(scope: &str, value: &str, subject: &BanSubject<'_>) -> bool {
    let scope = normalize_scope(scope);
    match scope.as_str() {
        "ip" => !subject.ip.is_empty() && subject.ip == value,
        "name" => {
            let Some(n) = sanitize_display_name(subject.name) else {
                return false;
            };
            let Some(v) = sanitize_display_name(value) else {
                return false;
            };
            eq_ci(&n, &v)
        }
        "name_regex" => {
            let Some(n) = sanitize_display_name(subject.name) else {
                return false;
            };
            glob_ci(value, &n)
        }
        "fingerprint" => !subject.fingerprint.is_empty() && eq_ci(subject.fingerprint, value),
        "ticket_id" => !subject.ticket_id.is_empty() && subject.ticket_id == value,
        "turn_user" => !subject.turn_user.is_empty() && subject.turn_user == value,
        "resume_token" => !subject.resume_token.is_empty() && subject.resume_token == value,
        "auth_key" => {
            // Collective match for “credentials-like” identifiers (IWAD fp, ticket, TURN, resume).
            [
                subject.fingerprint,
                subject.ticket_id,
                subject.turn_user,
                subject.resume_token,
            ]
            .iter()
            .any(|k| !k.is_empty() && (*k == value || eq_ci(k, value)))
        }
        _ => false,
    }
}

/// Find the first active ban that matches `subject`.
#[must_use]
pub fn find_ban_hit(bans: &[Ban], subject: &BanSubject<'_>) -> Option<BanHit> {
    let now = now_secs();
    for ban in bans {
        if !active(ban, now) {
            continue;
        }
        if match_scope(&ban.scope, ban.value.trim(), subject) {
            return Some(BanHit {
                id: ban.id.clone(),
                scope: normalize_scope(&ban.scope),
                reason: ban.reason.clone(),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ban(scope: &str, value: &str) -> Ban {
        Ban {
            id: "b1".into(),
            scope: scope.into(),
            value: value.into(),
            reason: "test".into(),
            exp: 0,
        }
    }

    #[test]
    fn name_exact_and_glob() {
        let bans = vec![ban("name", "Griefer"), ban("name_regex", "bot*")];
        assert!(
            find_ban_hit(
                &bans,
                &BanSubject {
                    name: "griefer",
                    ..Default::default()
                }
            )
            .is_some()
        );
        assert!(
            find_ban_hit(
                &bans,
                &BanSubject {
                    name: "Bot9000",
                    ..Default::default()
                }
            )
            .is_some()
        );
        assert!(
            find_ban_hit(
                &bans,
                &BanSubject {
                    name: "alice",
                    ..Default::default()
                }
            )
            .is_none()
        );
    }

    #[test]
    fn auth_key_matches_ticket_or_fp() {
        let bans = vec![ban("auth_key", "deadbeef")];
        assert!(
            find_ban_hit(
                &bans,
                &BanSubject {
                    fingerprint: "deadbeef",
                    ..Default::default()
                }
            )
            .is_some()
        );
        assert!(
            find_ban_hit(
                &bans,
                &BanSubject {
                    ticket_id: "deadbeef",
                    ..Default::default()
                }
            )
            .is_some()
        );
    }

    #[test]
    fn expired_ignored() {
        let mut b = ban("ip", "1.2.3.4");
        b.exp = 1; // long past
        assert!(
            find_ban_hit(
                &[b],
                &BanSubject {
                    ip: "1.2.3.4",
                    ..Default::default()
                }
            )
            .is_none()
        );
    }
}
