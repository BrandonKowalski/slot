//! The open cart's faces, built off the render thread. On the H700 a board takes the better part
//! of half a second to rasterise, which on the frame loop is half a second of frozen shelf, so
//! the frontend asks for the highlighted cart's faces as the caret lands and uploads them when
//! they come back. Only the newest request matters: a caret that has moved on has no use for the
//! cart it passed.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use slot_store::Cart;
use slot_ui::{board_face, cart_face, padded, CartFace, TURN_PAD};

pub struct BuiltFaces {
    pub stem: String,
    pub board: CartFace,
    pub lid: CartFace,
}

pub struct FaceBuilder {
    requests: Sender<Cart>,
    built: Receiver<BuiltFaces>,
}

impl FaceBuilder {
    pub fn spawn() -> Self {
        let (requests, inbox) = mpsc::channel::<Cart>();
        let (outbox, built) = mpsc::channel();
        // Named and built the way every other worker in this crate is (`link_start`,
        // `rewind`, `emu`, `audio/host`): a `Builder` rather than the bare `thread::spawn`, so
        // a `ps`/`top` on the device names this thread instead of just another anonymous one.
        let spawned = thread::Builder::new()
            .name("slot-faces".into())
            .spawn(move || {
                while let Ok(mut cart) = inbox.recv() {
                    // Straight to the newest: the requests before it were for carts the caret
                    // has already left.
                    while let Ok(newer) = inbox.try_recv() {
                        cart = newer;
                    }
                    let faces = BuiltFaces {
                        stem: cart.stem.clone(),
                        board: board_face(&cart),
                        lid: padded(&cart_face(&cart), TURN_PAD),
                    };
                    if outbox.send(faces).is_err() {
                        return;
                    }
                }
            });
        // A thread that never started leaves both ends of `inbox`/`outbox` dropped with it, so
        // `request` below sends into a channel nobody drains and `take` only ever sees it
        // disconnected — the same shape as a worker too far behind to answer in time. Task 10's
        // picker already waits on that and gives up after `FACES_WAIT_MS`, so this is reported
        // rather than turned into a panic that would take the whole frontend down with it.
        if let Err(e) = spawned {
            eprintln!("slot: faces: worker thread failed to start: {e}");
        }
        FaceBuilder { requests, built }
    }

    pub fn request(&self, cart: Cart) {
        // A worker that has gone has nothing to build with; the picker's own wait gives up.
        let _ = self.requests.send(cart);
    }

    /// The newest build finished since the last call, if any.
    pub fn take(&self) -> Option<BuiltFaces> {
        let mut newest = None;
        while let Ok(faces) = self.built.try_recv() {
            newest = Some(faces);
        }
        newest
    }
}
