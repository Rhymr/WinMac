use datamuse_api_rs::{DatamuseClient, EndPoint, RelatedType, Vocabulary};
use gtk::prelude::*;
use gtk::{Box as GtkBox, Frame, Label, ListBox, Orientation, ScrolledWindow, SearchEntry, pango};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::rc::Rc;
use tokio::runtime::Runtime;

/// Boxed callback fired on collapse/expand — factored out purely to keep
/// the field/local declarations under clippy's type-complexity threshold.
type ToggleCallback = Rc<RefCell<Option<Box<dyn Fn(bool)>>>>;

#[derive(Clone)]
pub struct RhymeSearch {
    frame: Frame,
    collapsed: Rc<Cell<bool>>,
    on_toggle: ToggleCallback,
}

impl Default for RhymeSearch {
    fn default() -> Self {
        Self::new()
    }
}

impl RhymeSearch {
    pub fn new() -> Self {
        // Create a vertical container for the input field and results
        let container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .css_classes(vec!["rhyme-search-body"])
            .spacing(5)
            .build();

        // Search row — a single search field (magnifier + clear built in);
        // Enter runs the lookup. No separate submit button, JetBrains-style.
        let input_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .css_classes(vec!["rhyme-search-input"])
            .spacing(5)
            .build();

        let word_input = SearchEntry::builder()
            .placeholder_text("Find rhymes for a word\u{2026}")
            .hexpand(true)
            .build();

        input_box.append(&word_input);

        // Create a ListBox to display rhyming words
        let rhyming_words_list = ListBox::builder()
            .css_classes(vec!["rhyme-results"])
            .margin_top(0)
            .margin_bottom(0)
            .margin_start(0)
            .margin_end(0)
            .selection_mode(gtk::SelectionMode::None)
            .build();
        let rhyming_words_list_cloned = rhyming_words_list.clone();
        let entry_cloned = word_input.clone();
        let entry_for_activate = entry_cloned.clone();

        // Wrap the rhyming words list in a ScrolledWindow for consistency
        let scrolled_window = ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .margin_top(0)
            .margin_bottom(0)
            .margin_start(0)
            .margin_end(0)
            .child(&rhyming_words_list)
            .build();

        // Plain header strip — a title only. Show/hide is driven solely by
        // the bottom stripe (see app::chrome::bottom_stripe); the panel has
        // no second click-to-collapse layer of its own.
        let header = GtkBox::new(Orientation::Horizontal, 6);
        header.set_css_classes(&["rhyme-search-header"]);

        let title = Label::new(Some("Rhyme Search"));
        title.set_css_classes(&["rhyme-search-title"]);

        header.append(&title);
        container.set_visible(false);

        // Create a `Frame` to match the styling of `TextEditor`
        let frame = Frame::builder()
            .child(&container)
            .css_classes(vec!["rhyme-search-container"])
            .build();
        frame.set_label_widget(Some(&header));

        let collapsed = Rc::new(Cell::new(true));
        let on_toggle: ToggleCallback = Rc::new(RefCell::new(None));

        let handle_submit = move |input: &str| {
            if !input.is_empty()
                && let Some(word) = entry_cloned.text().as_str().split_whitespace().next()
            {
                // Clear old results
                let mut child = rhyming_words_list_cloned.first_child();
                while let Some(widget) = child {
                    child = widget.next_sibling();
                    rhyming_words_list_cloned.remove(&widget);
                }

                // Fetch rhyming words using the rhyme API
                let rhymes = fetch_rhymes(word);

                // Group words by syllables from API result
                let mut syllable_groups: BTreeMap<usize, Vec<String>> = BTreeMap::new();
                for rhyme in rhymes {
                    syllable_groups.entry(rhyme.1).or_default().push(rhyme.0);
                }

                // Add new results grouped by headers and word lists
                for (syllable_count, words) in syllable_groups {
                    // Add header for the syllable group
                    let header_markup = format!(
                        "<span font_weight='bold' color='#e1e1e1'>{} Syllable{}</span>",
                        syllable_count,
                        if syllable_count == 1 { "" } else { "s" }
                    );
                    let header_label = Label::new(None);
                    header_label.set_markup(&header_markup);
                    header_label.set_halign(gtk::Align::Start);
                    header_label.set_margin_start(10);
                    header_label.set_margin_end(10);
                    header_label.set_margin_top(10);
                    header_label.set_margin_bottom(5);
                    rhyming_words_list_cloned.append(&header_label);

                    // Combine all words in the group into a single comma-separated string
                    let words_combined = words.join(", ");
                    let words_markup = format!("<span color='#ffffff'>{}</span>", words_combined);
                    let words_label = Label::new(None);
                    words_label.set_markup(&words_markup);
                    words_label.set_wrap(true);
                    words_label.set_halign(gtk::Align::Start);
                    words_label.set_margin_start(10);
                    words_label.set_margin_end(10);
                    words_label.set_margin_bottom(10);
                    words_label.set_wrap_mode(pango::WrapMode::WordChar);
                    rhyming_words_list_cloned.append(&words_label);
                }
            }
        };

        // Enter runs the lookup.
        entry_for_activate.connect_activate(move |entry| {
            handle_submit(entry.text().as_str());
        });

        // Create the container layout
        container.append(&input_box);
        container.append(&scrolled_window);

        // Set up the frame
        frame.set_child(Some(&container));

        Self {
            frame,
            collapsed,
            on_toggle,
        }
    }

    pub fn get_widget(&self) -> &Frame {
        &self.frame
    }

    pub fn is_collapsed(&self) -> bool {
        self.collapsed.get()
    }

    /// Show or hide the panel's body (the input + results), independent of
    /// the frame itself — used when the panel is docked at the bottom and
    /// its whole frame is toggled by the bottom stripe.
    pub fn set_expanded(&self, expanded: bool) {
        self.collapsed.set(!expanded);
        if let Some(container) = self.frame.child() {
            container.set_visible(expanded);
        }
    }

    /// Fires whenever the panel is collapsed/expanded, so the surrounding
    /// layout can give the freed space back to its neighbor — a Paned
    /// doesn't automatically resize the split just because a child's
    /// content was hidden.
    pub fn connect_toggle(&self, callback: impl Fn(bool) + 'static) {
        self.on_toggle.replace(Some(Box::new(callback)));
    }
}

// Function to simulate fetching rhyming words from an API.
// Replace this with an actual API call in a real application.
fn fetch_rhymes(word: &str) -> Vec<(String, usize)> {
    let rt = Runtime::new().expect("Failed to create a Tokio runtime");

    rt.block_on(async {
        let client = DatamuseClient::new();

        // TODO: Let user filter types in their results
        // Create a vector to hold all requests
        let requests = vec![
            client
                .new_query(Vocabulary::EnglishWiki, EndPoint::Words)
                .related(RelatedType::Rhyme, word),
            client
                .new_query(Vocabulary::EnglishWiki, EndPoint::Words)
                .related(RelatedType::ApproximateRhyme, word),
            client
                .new_query(Vocabulary::EnglishWiki, EndPoint::Words)
                .related(RelatedType::Homophones, word),
            client
                .new_query(Vocabulary::EnglishWiki, EndPoint::Words)
                .sounds_like(word),
        ];

        let mut unique_words: HashMap<String, usize> = HashMap::new(); // Use a HashMap to store unique words

        for request in requests {
            match request.list().await {
                Ok(word_list) => {
                    for word_data in word_list {
                        let word = word_data.word;
                        let syllables = word_data.num_syllables.unwrap_or(0); // Assume API provides `syllables`
                        if syllables > 0 {
                            unique_words.insert(word, syllables); // Insert into HashMap
                        }
                    }
                }
                Err(_) => continue, // Ignore errors and continue with the next request
            }
        }

        // Convert HashMap back to Vec
        unique_words.into_iter().collect::<Vec<(String, usize)>>()
    })
}
