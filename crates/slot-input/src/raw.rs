#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Btn {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
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
