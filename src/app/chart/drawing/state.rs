//! The live copy of the drawings: a gpui entity every chart reads and edits, so a drawing made on
//! one chart shows at once on the others that show the same symbol.

use gpui::Context;
use wyck::config::DocumentStore;

use super::book::Book;
use super::model::DrawingsDoc;
use crate::app::workspace::Saver;

const DOCUMENT: &str = "drawings";

pub struct Drawings {
    book: Book,
    saver: Saver<DrawingsDoc>,
    /// The revision of the book that was last handed to the saver.
    saved_revision: u64,
}

impl Drawings {
    /// Writes what waits to be saved now, for what is about to read the file (a backup).
    pub fn flush(&self) {
        self.saver.flush();
    }

    /// Reads the saved drawings (repairing them) and arranges for the last changes to be written
    /// when the app quits.
    pub fn new(store: DocumentStore, cx: &mut Context<Self>) -> Self {
        let book = Book::from_doc(store.load_or_default::<DrawingsDoc>(DOCUMENT));
        cx.on_app_quit(|this, _cx| {
            this.saver.flush();
            async {}
        })
        .detach();
        Self {
            saved_revision: book.revision(),
            book,
            saver: Saver::new(store, DOCUMENT),
        }
    }

    pub fn book(&self) -> &Book {
        &self.book
    }

    /// Changes the book, tells every chart to redraw, and saves if the change is one to keep.
    pub fn edit<R>(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Book) -> R) -> R {
        let result = change(&mut self.book);
        if self.book.revision() != self.saved_revision {
            self.saved_revision = self.book.revision();
            self.saver.schedule(self.book.to_doc());
        }
        cx.notify();
        result
    }
}
