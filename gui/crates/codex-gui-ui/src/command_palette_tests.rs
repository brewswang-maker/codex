//! Unit coverage of the palette filter and cursor.

use super::filtered_actions;
use super::move_selection;
use crate::state::PaletteState;

#[test]
fn an_empty_query_lists_every_action() {
    let actions = filtered_actions("");
    assert!(!actions.is_empty());
}

#[test]
fn the_filter_is_a_case_insensitive_subsequence_match() {
    let labels = |query: &str| -> Vec<String> {
        filtered_actions(query)
            .into_iter()
            .map(|action| action.label.to_string())
            .collect()
    };

    assert!(labels("open fld").first().is_some_and(|label| label.contains("Open folder")));
    // Subsequence, not substring: "nwtch" still reaches "New thread"? No —
    // but "nwtr" does (N-w-T-r across words).
    assert!(labels("nwtr").iter().any(|label| label.contains("New thread")));
    // Case-insensitive.
    assert!(labels("SETTINGS").iter().any(|label| label.contains("Settings")));
    // Nonsense matches nothing.
    assert!(labels("zzzz").is_empty());
}

#[test]
fn the_cursor_stays_within_the_filtered_list() {
    let mut palette = PaletteState {
        open: true,
        query: String::new(),
        selected: 0,
    };

    move_selection(&mut palette, 1);
    assert_eq!(palette.selected, 1, "down moves within bounds");

    // Moving far past the end clamps to the last row.
    for _ in 0..50 {
        move_selection(&mut palette, 1);
    }
    let last = filtered_actions("").len() - 1;
    assert_eq!(palette.selected, last);

    // Moving above the top clamps to the first row.
    for _ in 0..50 {
        move_selection(&mut palette, -1);
    }
    assert_eq!(palette.selected, 0);

    // An empty result resets the cursor.
    palette.query = String::from("zzzz");
    move_selection(&mut palette, 1);
    assert_eq!(palette.selected, 0);
}
