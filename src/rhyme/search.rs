use datamuse_api_rs::{DatamuseClient, EndPoint, RelatedType, Vocabulary};
use futures_util::StreamExt;
use gtk::prelude::*;
use gtk::{Box as GtkBox, Frame, Label, ListBox, Orientation, ScrolledWindow, SearchEntry, pango};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::rc::Rc;
use tokio::runtime::Runtime;

/// Outcome of one rhyme lookup.
enum LookupResult {
    /// `(word, syllable count)` pairs.
    Words(Vec<(String, usize)>),
    /// Lookup couldn't complete (offline, timed out, …).
    Failed,
}

/// Boxed callback fired on collapse/expand — factored out purely to keep
/// the field/local declarations under clippy's type-complexity threshold.
type ToggleCallback = Rc<RefCell<Option<Box<dyn Fn(bool)>>>>;

#[derive(Clone)]
pub struct RhymeSearch {
    frame: Frame,
    collapsed: Rc<Cell<bool>>,
    on_toggle: ToggleCallback,
    word_input: SearchEntry,
    /// Trailing action slot in the header — the dock adds a minimise button.
    header_actions: GtkBox,
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

        // Plain header strip — a title plus a trailing action slot the dock
        // fills with a minimise button.
        let header = GtkBox::new(Orientation::Horizontal, 6);
        header.set_css_classes(&["rhyme-search-header"]);

        let title = Label::new(Some("Rhyme Search"));
        title.set_css_classes(&["rhyme-search-title"]);
        title.set_hexpand(true);
        title.set_halign(gtk::Align::Start);

        let header_actions = GtkBox::new(Orientation::Horizontal, 2);

        header.append(&title);
        header.append(&header_actions);
        container.set_visible(false);

        // Create a `Frame` to match the styling of `TextEditor`
        let frame = Frame::builder()
            .child(&container)
            .css_classes(vec!["rhyme-search-container"])
            .build();
        frame.set_label_widget(Some(&header));

        let collapsed = Rc::new(Cell::new(true));
        let on_toggle: ToggleCallback = Rc::new(RefCell::new(None));

        // Bumped on every submit so a slow, superseded lookup's result is
        // dropped rather than overwriting a newer one.
        let generation = Rc::new(Cell::new(0u64));

        let handle_submit = move |input: &str| {
            let Some(word) = input.split_whitespace().next().map(str::to_string) else {
                return;
            };
            if word.is_empty() {
                return;
            }

            let submit_id = generation.get().wrapping_add(1);
            generation.set(submit_id);

            clear_results(&rhyming_words_list_cloned);
            rhyming_words_list_cloned.append(&status_label(&format!(
                "Searching for \u{201c}{word}\u{201d}\u{2026}"
            )));

            // The lookup (a Tokio runtime + Datamuse HTTP) runs on a worker
            // thread; the result comes back to the UI over a channel.
            let (sender, mut receiver) = futures_channel::mpsc::unbounded::<LookupResult>();
            std::thread::spawn(move || {
                let _ = sender.unbounded_send(fetch_rhymes(&word));
            });

            let list = rhyming_words_list_cloned.clone();
            let generation = generation.clone();
            glib::MainContext::default().spawn_local(async move {
                let Some(result) = receiver.next().await else {
                    return;
                };
                // A newer lookup started while this one was in flight.
                if generation.get() != submit_id {
                    return;
                }
                clear_results(&list);
                match result {
                    LookupResult::Failed => {
                        list.append(&status_label(
                            "Lookup failed \u{2014} check your connection.",
                        ));
                    }
                    LookupResult::Words(words) if words.is_empty() => {
                        list.append(&status_label("No rhymes found."));
                    }
                    LookupResult::Words(words) => render_results(&list, words),
                }
            });
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
            word_input,
            header_actions,
        }
    }

    pub fn get_widget(&self) -> &Frame {
        &self.frame
    }

    /// The trailing action area of the header, for the dock to drop a
    /// minimise button into.
    pub fn header_actions(&self) -> GtkBox {
        self.header_actions.clone()
    }

    pub fn is_collapsed(&self) -> bool {
        self.collapsed.get()
    }

    /// Put `word` in the search box (does not run the lookup — the user
    /// presses Enter). Used to seed it from the editor's current
    /// selection while the panel is open.
    pub fn set_query(&self, word: &str) {
        self.word_input.set_text(word);
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

/// Remove every row from the results list.
fn clear_results(list: &ListBox) {
    let mut child = list.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        list.remove(&widget);
    }
}

/// A plain dim status row ("Searching…", "No rhymes found.", …).
fn status_label(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.set_halign(gtk::Align::Start);
    label.set_margin_start(10);
    label.set_margin_end(10);
    label.set_margin_top(10);
    label.set_margin_bottom(10);
    label.add_css_class("dim-label");
    label
}

/// Fill `list` with the rhyme results, grouped by syllable count.
fn render_results(list: &ListBox, words: Vec<(String, usize)>) {
    let mut groups: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (w, syllables) in words {
        groups.entry(syllables).or_default().push(w);
    }
    for (syllable_count, mut group) in groups {
        group.sort();
        let header = Label::new(None);
        header.set_markup(&format!(
            "<span font_weight='bold' color='#e1e1e1'>{} Syllable{}</span>",
            syllable_count,
            if syllable_count == 1 { "" } else { "s" }
        ));
        header.set_halign(gtk::Align::Start);
        header.set_margin_start(10);
        header.set_margin_end(10);
        header.set_margin_top(10);
        header.set_margin_bottom(5);
        list.append(&header);

        let words_label = Label::new(None);
        words_label.set_markup(&format!(
            "<span color='#ffffff'>{}</span>",
            group.join(", ")
        ));
        words_label.set_wrap(true);
        words_label.set_halign(gtk::Align::Start);
        words_label.set_margin_start(10);
        words_label.set_margin_end(10);
        words_label.set_margin_bottom(10);
        words_label.set_wrap_mode(pango::WrapMode::WordChar);
        list.append(&words_label);
    }
}

/// Query Datamuse for words that rhyme with `word`. Blocking — run on a
/// worker thread (see `handle_submit`). Never panics; a build/timeout/
/// transport failure yields [`LookupResult::Failed`].
fn fetch_rhymes(word: &str) -> LookupResult {
    log::debug!("datamuse rhyme lookup for {word:?}");
    let Ok(rt) = Runtime::new() else {
        log::warn!("rhyme lookup for {word:?}: could not build a tokio runtime");
        return LookupResult::Failed;
    };

    rt.block_on(async {
        let lookup = async {
            let client = DatamuseClient::new();
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

            let mut unique: HashMap<String, usize> = HashMap::new();
            let mut any_ok = false;
            for request in requests {
                if let Ok(word_list) = request.list().await {
                    any_ok = true;
                    for wd in word_list {
                        let syllables = wd.num_syllables.unwrap_or(0);
                        if syllables > 0 {
                            unique.insert(wd.word, syllables);
                        }
                    }
                }
            }
            (any_ok, unique)
        };

        match tokio::time::timeout(crate::config::rhyme_lookup_timeout(), lookup).await {
            Ok((true, unique)) => {
                log::debug!("datamuse returned {} rhymes for {word:?}", unique.len());
                LookupResult::Words(unique.into_iter().collect())
            }
            // every request failed, or we timed out
            _ => {
                log::warn!("rhyme lookup for {word:?} failed or timed out (offline?)");
                LookupResult::Failed
            }
        }
    })
}
