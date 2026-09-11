//! The link screen's artwork, built once on its own thread. Rasterising it on the H700 costs
//! seconds, and anything built on the frame loop costs the animation that frame.

use std::sync::mpsc::{channel, Receiver};

use slot_ui::{link_art, LinkArt};

pub struct LinkArtBuilder {
    built: Receiver<LinkArt>,
}

impl LinkArtBuilder {
    pub fn spawn() -> Self {
        let (tx, built) = channel();
        let spawned = std::thread::Builder::new()
            .name("slot-link-art".into())
            .spawn(move || {
                let _ = tx.send(link_art());
            });
        // No art is a link screen with text and keys only, not a boot failure.
        if let Err(e) = spawned {
            eprintln!("slot: link art worker: {e}");
        }
        LinkArtBuilder { built }
    }

    /// The art, the one time it is ready.
    pub fn take(&self) -> Option<LinkArt> {
        self.built.try_recv().ok()
    }
}
