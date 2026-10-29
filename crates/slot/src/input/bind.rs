use slot_input::{Action, Btn, Chord, Mod};
use slot_store::{Binding, Device, Modifier, Shortcut};

const PAIRS: [(Device, Btn); 14] = [
    (Device::Up, Btn::Up),
    (Device::Down, Btn::Down),
    (Device::Left, Btn::Left),
    (Device::Right, Btn::Right),
    (Device::A, Btn::A),
    (Device::B, Btn::B),
    (Device::X, Btn::X),
    (Device::Y, Btn::Y),
    (Device::L1, Btn::L1),
    (Device::R1, Btn::R1),
    (Device::L2, Btn::L2),
    (Device::R2, Btn::R2),
    (Device::Select, Btn::Select),
    (Device::Start, Btn::Start),
];

pub fn device_btn(d: Device) -> Btn {
    PAIRS
        .iter()
        .find(|(x, _)| *x == d)
        .map_or(Btn::A, |(_, b)| *b)
}

pub fn btn_device(b: Btn) -> Option<Device> {
    PAIRS.iter().find(|(_, x)| *x == b).map(|(d, _)| *d)
}

pub(super) fn chord(binding: Binding) -> Chord {
    Chord {
        modifier: binding.modifier.map(|m| match m {
            Modifier::Select => Mod::Select,
            Modifier::Menu => Mod::Menu,
            Modifier::Button(d) => Mod::Button(device_btn(d)),
        }),
        button: device_btn(binding.button),
    }
}

pub(super) fn shortcut_action(s: Shortcut) -> Action {
    match s {
        Shortcut::SaveState => Action::SaveState,
        Shortcut::LoadState => Action::LoadState,
        Shortcut::Screenshot => Action::Screenshot,
        Shortcut::BrightnessUp => Action::BrightnessUp,
        Shortcut::BrightnessDown => Action::BrightnessDown,
        Shortcut::BlueLightDown => Action::BlueLightDown,
        Shortcut::BlueLightUp => Action::BlueLightUp,
        Shortcut::ColorCorrection => Action::ColourCorrectionToggle,
        Shortcut::PaletteBack => Action::PalettePrev,
        Shortcut::PaletteNext => Action::PaletteNext,
        Shortcut::ShaderNext => Action::ShaderNext,
        Shortcut::ShaderBack => Action::ShaderPrev,
        Shortcut::FastForward => Action::FfStart,
        Shortcut::FastForwardToggle => Action::FfToggle,
        Shortcut::Rewind => Action::RewindStart,
        Shortcut::SunlightUp => Action::SunlightUp,
        Shortcut::SunlightDown => Action::SunlightDown,
        Shortcut::Reset => Action::Reset,
        Shortcut::Guide => Action::Guide,
    }
}
