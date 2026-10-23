# Customization

## Cart labels

Drop a `.png` into the `Labels/{Platform}` folder with the same name as the ROM. 

e.g. `Labels/GBA/Metroid Fusion.png` goes with `Games/GBA/Metroid Fusion.gba`.

| Cart | Shape | Suggested size |
| --- | --- | --- |
| GBA | 1.94:1 | 1280×661 |
| Game Boy | 1.12:1 | 1024×914 |
| Game Boy Color | 1.12:1 | 1024×914 |

slot will cache labels on the next boot after adding labels.

You don't have to make these by hand and can use the [Cart Studio](https://studio.slot-cfw.fyi) to fetch and apply labels.

## Cart shells

You can also customize the color and appearance of the cart shell.

To pick your own, add a line to `Labels/cart_shell.txt`:

```
Metroid Fusion = auto 6a3fa0 solid
```

That's the game's file name, then three words:

1. **Shape**: `auto`, `notched` or `rounded`. Only matters for Game Boy carts. `auto` uses the notched cart for original Game Boy games and the rounded one for Color-only games.
2. **Color**: six hex digits, no `#`.
3. **Finish**: `solid`, `clear` or `glitter`.

Keep all three options lowercase. A line slot can't read is skipped.

## Wallpapers

Drop a `.png` into `Wallpapers/` and pick it under Wallpaper in Settings → General. 

Random picks one at boot.

| Device | Suggested Size |
| --- | --- |
| RG SP, RG34XX, RG34XXSP | 720×480 |
| RG35XXSP, RG35XX Plus, RG35XX-H, RG28XX | 640×480 |

Other sizes are scaled to fill the screen and cropped from the center. 

If your wallpaper is too bright, the Wallpaper Tint setting can darken it a bit to make everything easier to read.

## Bezels

Drop a `.png` into `Bezels/GBA/`, `Bezels/GBC/` or `Bezels/GB/` and pick it under Bezel in that system's Screen settings.

The bezel is drawn over the game, so leave a transparent hole where the game goes:

| Device | Bezel size | Game area |
| --- | --- | --- |
| RG SP, RG34XX, RG34XXSP | 720×480 | 480×432 at 120, 24 |
| RG35XXSP, RG35XX Plus, RG35XX-H, RG28XX | 640×480 | 427×384 at 107, 48 |

GBA bezels only show on the 640×480 devices, where the game is 640×427 at 0, 27. On the 720×480 devices a GBA game fills the screen.


## Boot logo

Put a `.bmp` at `Config/bootlogo.bmp`. slot hands it to the device when it boots, so you'll see it the next time you power on.

slot copies the file as it is, without resizing it, so make it the same size and format as the logo your device already shows.

Delete `Config/bootlogo.bmp` to go back to the slot logo.

## BIOS

slot doesn't need any BIOS files. If you have your own, drop them into `BIOS/` and you'll get the real boot animation when a game starts.

- `gba_bios.bin`
- `gbc_bios.bin`
- `gb_bios.bin`

If you link with mGBA, both devices need the same BIOS setup. See [Linking Up](linking.md).
