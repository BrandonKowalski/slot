#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Btn {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    /// Never reach the core: the GBA has neither button. They exist so the switcher has
    /// buttons of its own for the undo and the delete.
    X,
    Y,
    L1,
    R1,
    L2,
    R2,
    Start,
    Select,
    Menu,
    VolUp,
    VolDown,
    Power,
    Lid,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RawEvent {
    Down(Btn),
    Up(Btn),
}

pub type Millis = u64;

pub trait InputSource {
    fn poll(&mut self, now: Millis) -> Vec<RawEvent>;
}
