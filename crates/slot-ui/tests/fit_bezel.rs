use slot_ui::fit_game_bezel_face;

const W: usize = 720;
const H: usize = 480;
// The fitted Game Boy picture spans about x 93.3..626.7 of the 720 wide screen.
const PICTURE: std::ops::Range<usize> = 93..627;

#[test]
fn the_fit_bezels_only_draw_in_the_side_bars() {
    for colour in [false, true] {
        let rgba = fit_game_bezel_face(colour).expect("decode the fit bezel");
        assert_eq!(rgba.len(), W * H * 4);
        let alpha = |x: usize, y: usize| rgba[(y * W + x) * 4 + 3];
        let over = (0..H)
            .flat_map(|y| PICTURE.map(move |x| (x, y)))
            .filter(|&(x, y)| alpha(x, y) > 0)
            .count();
        assert_eq!(over, 0, "colour {colour}: the bezel covers the picture");
        for (side, xs) in [("left", 0..93), ("right", 627..W)] {
            let drawn = (0..H)
                .flat_map(|y| xs.clone().map(move |x| (x, y)))
                .filter(|&(x, y)| alpha(x, y) == 255)
                .count();
            assert!(
                drawn > 100,
                "colour {colour}: nothing drawn in the {side} bar"
            );
        }
    }
}
