//! Render the Wi-Fi screens without a device or a graphics context.
use slot::wifi::{Network, Security};
use slot::wifi_menu::WifiMenu;
use slot_input::Btn;

fn main() {
    let output = std::path::PathBuf::from(std::env::args().nth(1).expect("output directory"));
    std::fs::create_dir_all(&output).unwrap();
    let mut menu = WifiMenu::default();
    menu.status = "Connected | IP 192.168.1.42".into();
    menu.networks = vec![
        Network {
            ssid: "Home Wi-Fi".into(),
            signal: -32,
            security: Security::Personal,
        },
        Network {
            ssid: "Guest".into(),
            signal: -50,
            security: Security::Open,
        },
    ];
    menu.row = 4;
    for name in ["wifi-networks.png", "wifi-password.png"] {
        let face = menu.face();
        let file = std::fs::File::create(output.join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, face.w, face.h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&face.rgba)
            .unwrap();
        menu.input(Btn::A);
        menu.input(Btn::A);
        menu.input(Btn::Right);
        menu.input(Btn::A);
    }
}
