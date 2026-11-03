//! Line-special name → number (BD3 specials-as-actions).

/// Resolve a Hexen/ZDoom special name (case-insensitive).
///
/// The table lives in `room-zscript` so the compiler and the playsim share it.
#[must_use]
pub fn special_number(name: &str) -> Option<u16> {
    room_zscript::special_number(name)
}

/// True when `name` is a named-ACS state action (`ACS_NamedExecute*`).
#[must_use]
pub fn is_named_acs_action(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "acs_namedexecute" | "acs_namedexecutealways" | "acs_namedexecutewithresult"
    )
}

/// True when `name` is a known line special usable as a state action.
#[must_use]
pub fn is_special_action(name: &str) -> bool {
    is_named_acs_action(name) || special_number(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::specials::num;

    #[test]
    fn resolves_bd_hot_specials() {
        assert_eq!(special_number("ACS_Execute"), Some(num::ACS_EXECUTE));
        assert_eq!(
            special_number("acs_executealways"),
            Some(num::ACS_EXECUTE_ALWAYS)
        );
        assert!(is_named_acs_action("ACS_NamedExecuteAlways"));
        assert!(is_special_action("ACS_NamedExecuteAlways"));
        assert_eq!(special_number("Radius_Quake"), Some(num::RADIUS_QUAKE));
        assert_eq!(special_number("Thing_Spawn"), Some(num::THING_SPAWN));
    }
}
