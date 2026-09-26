//! The composer's `@file` mention tracker.
//!
//! Pure and iced-free: the update loop feeds each composer change in,
//! receives a search query back while a mention token is live, hands the
//! latest `fuzzyFileSearch` hits over, and rewrites the token on a pick.

/// One pickable file from `fuzzyFileSearch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionHit {
    /// Absolute path; what a pick inserts into the composer.
    pub path: String,
    /// Display name the search reported.
    pub file_name: String,
}

/// Live state of the mention completion popup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mentions {
    open: bool,
    query: String,
    /// Byte offset of the leading `@` of the live token.
    token_start: usize,
    hits: Vec<MentionHit>,
    selected: usize,
}

impl Mentions {
    /// Re-derives the live token from the composer text and returns the
    /// query to search when it just changed. `None` means nothing to
    /// search: no live `@` token, or the same query already in flight.
    pub fn refresh(&mut self, composer: &str) -> Option<String> {
        let token_start = composer
            .char_indices()
            .filter(|(_, ch)| ch.is_whitespace())
            .map(|(index, ch)| index + ch.len_utf8())
            .next_back()
            .unwrap_or(0);
        let token = &composer[token_start..];
        let Some(query) = token.strip_prefix('@') else {
            self.close();
            return None;
        };

        let changed = !self.open || self.query != query;
        self.open = true;
        self.token_start = token_start;
        self.query = query.to_string();
        changed.then(|| query.to_string())
    }

    /// Whether the popup should render.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The live query, without the leading `@`.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The current hit list.
    pub fn hits(&self) -> &[MentionHit] {
        &self.hits
    }

    /// The highlighted row index.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Replaces the hit list with a fresh page, clamping the highlight.
    pub fn apply_hits(&mut self, hits: Vec<MentionHit>) {
        self.hits = hits;
        if self.selected >= self.hits.len() {
            self.selected = 0;
        }
    }

    /// Moves the highlight, wrapping at both ends.
    pub fn move_selection(&mut self, delta: i32) {
        if self.hits.is_empty() {
            return;
        }
        let len = self.hits.len() as i32;
        self.selected = (self.selected as i32 + delta).rem_euclid(len) as usize;
    }

    /// Highlights the row at `index` (a click); out-of-range is ignored.
    pub fn select(&mut self, index: usize) {
        if index < self.hits.len() {
            self.selected = index;
        }
    }

    /// Rewrites the live token into `@path ` and closes the popup; `false`
    /// when there is nothing to insert.
    pub fn insert_selected(&mut self, composer: &mut String) -> bool {
        let Some(hit) = self.hits.get(self.selected) else {
            return false;
        };
        let insertion = format!("@{} ", hit.path);
        composer.replace_range(self.token_start.., &insertion);
        self.close();
        true
    }

    /// Closes the popup without touching the composer (Esc).
    pub fn close(&mut self) {
        self.open = false;
        self.hits.clear();
    }
}
