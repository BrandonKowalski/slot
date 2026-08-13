//! What this binary is, for the about label. `slot-ui` cannot read any of it: these are this
//! crate's compile time environment, so they travel to the label as arguments.

/// Everything the label says about the build. `'static` throughout because all of it is baked
/// in by `build.rs` at compile time.
#[derive(Copy, Clone, Debug)]
pub struct Build {
    pub version: &'static str,
    pub hash: &'static str,
    pub dirty: bool,
    pub date: &'static str,
}

impl Build {
    pub fn current() -> Build {
        Build {
            version: env!("CARGO_PKG_VERSION"),
            hash: env!("SLOT_GIT_HASH"),
            dirty: env!("SLOT_GIT_DIRTY") == "1",
            date: env!("SLOT_BUILD_DATE"),
        }
    }

    /// What the barcode encodes. Upper case because Code 39 has none, and a hash outside its
    /// alphabet refuses to encode rather than scanning as something else.
    pub fn serial(&self) -> String {
        self.hash.to_uppercase()
    }

    /// The digit in the box beside the bars. The real label's is a check digit; this one is
    /// the only place a build from a modified tree admits to it.
    pub fn dirty_digit(&self) -> char {
        match self.dirty {
            true => '1',
            false => '0',
        }
    }
}
