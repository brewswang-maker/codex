//! Mention-tracker tests over plain composer strings.

use super::mentions::MentionHit;
use super::mentions::Mentions;
use pretty_assertions::assert_eq;

fn hit(path: &str) -> MentionHit {
    MentionHit {
        path: path.to_string(),
        file_name: path.rsplit('/').next().unwrap_or(path).to_string(),
    }
}

#[test]
fn refresh_opens_on_a_trailing_at_token() {
    let mut mentions = Mentions::default();

    assert_eq!(mentions.refresh("find @sr"), Some("sr".to_string()));
    assert!(mentions.is_open());
    assert_eq!(mentions.query(), "sr");
}

#[test]
fn refresh_requires_a_fresh_query() {
    let mut mentions = Mentions::default();

    assert_eq!(mentions.refresh("@src"), Some("src".to_string()));
    // An edit before the token leaves the live query unchanged.
    assert_eq!(mentions.refresh("go @src"), None);
    assert_eq!(mentions.refresh("go @lib"), Some("lib".to_string()));
}

#[test]
fn refresh_closes_without_a_live_token() {
    let mut mentions = Mentions::default();

    assert_eq!(mentions.refresh("@src"), Some("src".to_string()));
    assert_eq!(mentions.refresh("@src done"), None);
    assert!(!mentions.is_open());
    // Mid-word `@` (an email) never opens the popup.
    assert_eq!(mentions.refresh("mail a@b"), None);
    assert!(!mentions.is_open());
}

#[test]
fn insert_rewrites_the_token_and_closes() {
    let mut mentions = Mentions::default();
    assert_eq!(mentions.refresh("open @li"), Some("li".to_string()));
    mentions.apply_hits(vec![hit("/repo/src/lib.rs")]);

    let mut composer = String::from("open @li");
    assert!(mentions.insert_selected(&mut composer));
    assert_eq!(composer, "open @/repo/src/lib.rs ");
    assert!(!mentions.is_open());
}

#[test]
fn insert_without_hits_is_a_noop() {
    let mut mentions = Mentions::default();
    assert_eq!(mentions.refresh("@li"), Some("li".to_string()));

    let mut composer = String::from("@li");
    assert!(!mentions.insert_selected(&mut composer));
    assert_eq!(composer, "@li");
}

#[test]
fn selection_wraps_and_clamps_on_new_hits() {
    let mut mentions = Mentions::default();
    assert_eq!(mentions.refresh("@x"), Some("x".to_string()));
    mentions.apply_hits(vec![hit("/a"), hit("/b"), hit("/c")]);

    mentions.move_selection(1);
    assert_eq!(mentions.selected(), 1);
    mentions.move_selection(1);
    assert_eq!(mentions.selected(), 2);
    mentions.move_selection(1);
    assert_eq!(mentions.selected(), 0, "wraps forward");
    mentions.move_selection(-1);
    assert_eq!(mentions.selected(), 2, "wraps backward");

    // A shorter page resets the highlight.
    mentions.apply_hits(vec![hit("/only")]);
    assert_eq!(mentions.selected(), 0);
}

#[test]
fn select_highlights_a_clicked_row() {
    let mut mentions = Mentions::default();
    assert_eq!(mentions.refresh("@x"), Some("x".to_string()));
    mentions.apply_hits(vec![hit("/a"), hit("/b")]);

    mentions.select(1);
    assert_eq!(mentions.selected(), 1);

    // Out-of-range clicks leave the highlight alone.
    mentions.select(9);
    assert_eq!(mentions.selected(), 1);
}

#[test]
fn close_keeps_the_composer_text_untouched() {
    let mut mentions = Mentions::default();
    assert_eq!(mentions.refresh("@li"), Some("li".to_string()));
    mentions.apply_hits(vec![hit("/repo/src/lib.rs")]);

    mentions.close();
    assert!(!mentions.is_open());
    assert!(mentions.hits().is_empty());
}
