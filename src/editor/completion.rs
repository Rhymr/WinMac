use gio::prelude::*;
use glib::subclass::prelude::*;
use gtk::prelude::*;
use sourceview5::{
    CompletionCell, CompletionColumn, CompletionContext, CompletionProposal, CompletionProvider,
};
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;

/// The bundled dictionary, one lowercase word per line, already sorted so
/// prefix lookups can binary-search instead of scanning all ~274k entries.
const WORDS_TXT: &str = include_str!("../../assets/dictionary/words.txt");

fn dictionary() -> &'static [&'static str] {
    static WORDS: OnceLock<Vec<&'static str>> = OnceLock::new();
    WORDS.get_or_init(|| WORDS_TXT.lines().collect())
}

/// Words in `words` that start with `prefix`, relying on `words` being
/// sorted so the matches form one contiguous range.
fn prefix_matches<'a>(words: &'a [&'a str], prefix: &str) -> &'a [&'a str] {
    let start = words.partition_point(|w| *w < prefix);
    let len = words[start..].partition_point(|w| w.starts_with(prefix));
    &words[start..start + len]
}

/// Replace `store`'s contents with dictionary matches for `word` — shared by
/// `populate` (opens a completion session) and `refilter` (GtkSourceView's
/// completion engine calls this instead of `populate` again on every
/// keystroke after the first, to narrow the same session's list).
/// `min_prefix` is how many characters must be typed before suggesting;
/// `max` caps the list.
fn populate_store(store: &gio::ListStore, word: &str, min_prefix: usize, max: usize) {
    store.remove_all();
    if word.chars().count() >= min_prefix.max(1) {
        for w in prefix_matches(dictionary(), word).iter().take(max) {
            // Skip the trivial case where the typed word is already a
            // complete dictionary entry with no completion to offer.
            if *w != word {
                store.append(&WordProposal::new(w));
            }
        }
    }
}

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct WordProposal {
        pub word: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WordProposal {
        const NAME: &'static str = "RhymrWordProposal";
        type Type = super::WordProposal;
        type Interfaces = (CompletionProposal,);
    }

    impl ObjectImpl for WordProposal {}
    impl sourceview5::subclass::prelude::CompletionProposalImpl for WordProposal {}

    pub struct WordCompletionProvider {
        pub min_prefix: Cell<usize>,
        pub max_suggestions: Cell<usize>,
    }

    impl Default for WordCompletionProvider {
        fn default() -> Self {
            Self {
                min_prefix: Cell::new(2),
                max_suggestions: Cell::new(100),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WordCompletionProvider {
        const NAME: &'static str = "RhymrWordCompletionProvider";
        type Type = super::WordCompletionProvider;
        type Interfaces = (CompletionProvider,);
    }

    impl ObjectImpl for WordCompletionProvider {}

    impl sourceview5::subclass::prelude::CompletionProviderImpl for WordCompletionProvider {
        fn title(&self) -> Option<glib::GString> {
            Some("Dictionary".into())
        }

        fn priority(&self, _context: &CompletionContext) -> i32 {
            -1
        }

        fn is_trigger(&self, _iter: &gtk::TextIter, c: char) -> bool {
            c.is_alphanumeric()
        }

        fn populate(&self, context: &CompletionContext) -> Result<gio::ListModel, glib::Error> {
            let word = context.word().to_string().to_lowercase();
            let store = gio::ListStore::new::<super::WordProposal>();
            super::populate_store(
                &store,
                &word,
                self.min_prefix.get(),
                self.max_suggestions.get(),
            );
            Ok(store.upcast())
        }

        fn refilter(&self, context: &CompletionContext, model: &gio::ListModel) {
            let Some(store) = model.downcast_ref::<gio::ListStore>() else {
                return;
            };
            let word = context.word().to_string().to_lowercase();
            super::populate_store(
                store,
                &word,
                self.min_prefix.get(),
                self.max_suggestions.get(),
            );
        }

        fn populate_future(
            &self,
            context: &CompletionContext,
        ) -> Pin<Box<dyn Future<Output = Result<gio::ListModel, glib::Error>>>> {
            Box::pin(std::future::ready(self.populate(context)))
        }

        /// Tab and Enter both accept the highlighted proposal, in addition
        /// to whatever GTK already binds by default — Tab is the natural
        /// "take the suggestion" key for a word-completion popup.
        fn key_activates(
            &self,
            _context: &CompletionContext,
            _proposal: &CompletionProposal,
            keyval: gtk::gdk::Key,
            _state: gtk::gdk::ModifierType,
        ) -> bool {
            matches!(
                keyval,
                gtk::gdk::Key::Tab | gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter
            )
        }

        fn display(
            &self,
            _context: &CompletionContext,
            proposal: &CompletionProposal,
            cell: &CompletionCell,
        ) {
            if cell.column() == CompletionColumn::TypedText
                && let Some(word) = proposal.downcast_ref::<super::WordProposal>()
            {
                cell.set_text(Some(&word.word()));
            }
        }

        fn activate(&self, context: &CompletionContext, proposal: &CompletionProposal) {
            let Some(word) = proposal.downcast_ref::<super::WordProposal>() else {
                return;
            };
            let Some(buffer) = context.buffer() else {
                return;
            };
            let Some((mut start, mut end)) = context.bounds() else {
                return;
            };
            buffer.delete(&mut start, &mut end);
            buffer.insert(&mut start, &word.word());

            // Explicitly close instead of leaving the (now stale) proposal
            // list lingering on screen after a selection.
            if let Some(completion) = context.completion() {
                completion.hide();
            }
        }
    }
}

glib::wrapper! {
    pub struct WordProposal(ObjectSubclass<imp::WordProposal>)
        @implements CompletionProposal;
}

impl WordProposal {
    fn new(word: &str) -> Self {
        let obj: Self = glib::Object::new();
        obj.imp().word.replace(word.to_string());
        obj
    }

    fn word(&self) -> String {
        self.imp().word.borrow().clone()
    }
}

glib::wrapper! {
    pub struct WordCompletionProvider(ObjectSubclass<imp::WordCompletionProvider>)
        @implements CompletionProvider;
}

impl WordCompletionProvider {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Set how many characters must be typed before suggesting and the
    /// maximum number of proposals (Settings → Editor → Completions).
    pub fn set_limits(&self, min_prefix: usize, max_suggestions: usize) {
        self.imp().min_prefix.set(min_prefix.max(1));
        self.imp().max_suggestions.set(max_suggestions.max(1));
    }
}

impl Default for WordCompletionProvider {
    fn default() -> Self {
        Self::new()
    }
}
