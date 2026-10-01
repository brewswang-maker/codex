//! Single-line text truncation helpers.
//!
//! Iced has no built-in ellipsis, so labels that must never wrap (quest
//! rows, banner strips) pre-truncate here: the text is cut on a display
//! width budget where CJK/fullwidth characters count double, and an
//! ellipsis marks the cut.

/// The display width of one character: CJK and fullwidth forms take two
/// cells, everything else takes one.
fn char_width(c: char) -> usize {
    if is_wide(c) { 2 } else { 1 }
}

/// Whether the char renders as a double-width (fullwidth) cell.
fn is_wide(c: char) -> bool {
    let c = c as u32;
    matches!(c,
        0x1100..=0x115F   // Hangul Jamo
        | 0x2E80..=0x303E // CJK radicals, Kangxi, CJK symbols
        | 0x3041..=0x33FF // Hiragana..CJK compatibility
        | 0x3400..=0x4DBF // CJK ext A
        | 0x4E00..=0x9FFF // CJK unified
        | 0xA000..=0xA4CF // Yi
        | 0xAC00..=0xD7A3 // Hangul syllables
        | 0xF900..=0xFAFF // CJK compatibility ideographs
        | 0xFE30..=0xFE4F // CJK compatibility forms
        | 0xFF00..=0xFF60 // fullwidth forms
        | 0xFFE0..=0xFFE6 // fullwidth signs
        | 0x20000..=0x3FFFD // CJK ext B+
    )
}

/// The display width of a string under the CJK-aware rule.
pub fn display_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

/// Truncates to at most `max_width` display cells, appending an ellipsis
/// when anything was cut. Text that already fits is returned unchanged.
pub fn ellipsize(text: &str, max_width: usize) -> String {
    if display_width(text) <= max_width {
        return String::from(text);
    }
    const ELLIPSIS: &str = "…";
    let ellipsis_width = display_width(ELLIPSIS);
    let budget = max_width.saturating_sub(ellipsis_width);
    let mut width = 0usize;
    let mut out = String::new();
    for c in text.chars() {
        let w = char_width(c);
        if width + w > budget {
            break;
        }
        width += w;
        out.push(c);
    }
    out.push_str(ELLIPSIS);
    out
}

#[cfg(test)]
#[path = "text_fit_tests.rs"]
mod tests;
