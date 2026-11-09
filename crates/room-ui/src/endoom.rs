//! ENDOOM / ENDTEXT / ENDSTRF: the 80×25 text-mode quit screen.
//!
//! The lump is 4000 bytes of (CP437 character, VGA attribute) pairs. On quit
//! the desktop shell prints it to the terminal with ANSI colours when the
//! `showendoom` CVar is on (GZDoom shows it in a window).

/// Columns × rows of the text screen.
pub const ENDOOM_COLS: usize = 80;
/// Rows.
pub const ENDOOM_ROWS: usize = 25;

/// Code page 437, byte → char (control bytes use their glyph forms).
const CP437: &str = " ☺☻♥♦♣♠•◘○◙♂♀♪♫☼►◄↕‼¶§▬↨↑↓→←∟↔▲▼ !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~⌂ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■ ";

/// VGA colour index → ANSI 256-colour palette entry (EGA order differs from ANSI).
const VGA_TO_ANSI: [u8; 16] = [0, 4, 2, 6, 1, 5, 3, 7, 8, 12, 10, 14, 9, 13, 11, 15];

/// Lump names to try, in order (Doom, Heretic/Hexen, Strife).
pub const ENDOOM_LUMPS: [&str; 3] = ["ENDOOM", "ENDTEXT", "ENDSTRF"];

/// Decode one CP437 byte.
#[must_use]
pub fn cp437_char(b: u8) -> char {
    CP437.chars().nth(usize::from(b)).unwrap_or(' ')
}

/// Plain text (no colour), one line per row, trailing spaces trimmed.
#[must_use]
pub fn endoom_to_text(data: &[u8]) -> Option<String> {
    if data.len() < ENDOOM_COLS * ENDOOM_ROWS * 2 {
        return None;
    }
    let mut out = String::new();
    for row in data.chunks_exact(ENDOOM_COLS * 2).take(ENDOOM_ROWS) {
        let line: String = row.chunks_exact(2).map(|c| cp437_char(c[0])).collect();
        out.push_str(line.trim_end());
        out.push('\n');
    }
    Some(out)
}

/// ANSI-coloured rendering (foreground + background per cell, blink ignored).
#[must_use]
pub fn endoom_to_ansi(data: &[u8]) -> Option<String> {
    if data.len() < ENDOOM_COLS * ENDOOM_ROWS * 2 {
        return None;
    }
    let mut out = String::new();
    for row in data.chunks_exact(ENDOOM_COLS * 2).take(ENDOOM_ROWS) {
        let mut last: Option<u8> = None;
        for cell in row.chunks_exact(2) {
            let attr = cell[1] & 0x7f;
            if last != Some(attr) {
                let fg = VGA_TO_ANSI[usize::from(attr & 0x0f)];
                let bg = VGA_TO_ANSI[usize::from((attr >> 4) & 0x07)];
                out.push_str(&format!("\x1b[38;5;{fg};48;5;{bg}m"));
                last = Some(attr);
            }
            out.push(cp437_char(cell[0]));
        }
        out.push_str("\x1b[0m\n");
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endoom_decodes_cp437_and_colours() {
        let mut lump = vec![0u8; 4000];
        for cell in lump.chunks_exact_mut(2) {
            cell[0] = b' ';
            cell[1] = 0x07;
        }
        lump[0] = b'D';
        lump[1] = 0x4f; // white on red
        lump[2] = 0xdb; // full block
        lump[3] = 0x4f;
        let text = endoom_to_text(&lump).unwrap();
        assert!(text.starts_with("D\u{2588}\n"));
        assert_eq!(text.lines().count(), 25);
        let ansi = endoom_to_ansi(&lump).unwrap();
        assert!(ansi.starts_with("\x1b[38;5;15;48;5;1mD\u{2588}"));
        assert_eq!(cp437_char(1), '\u{263a}');
        assert_eq!(cp437_char(0xc4), '\u{2500}');
        assert!(endoom_to_text(&lump[..100]).is_none());
    }
}
