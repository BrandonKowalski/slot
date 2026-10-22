# Customization

## Cart labels

Drop a `.png` into the `Labels/` folder for that system, named after the game. `Labels/GBA/Metroid Fusion.png` goes with `Games/GBA/Metroid Fusion.gba`.

| Cart | Shape | For example |
| --- | --- | --- |
| GBA | 1.94:1 | 1280×661 |
| Game Boy | 1.13:1 | 1024×901 |
| Game Boy Color | 1.12:1 | 1024×901 |

The first boot after adding labels shows Caching New Labels while slot gets them ready. That only happens once per label.

You don't have to make these by hand. The [slot Cart Studio](https://studio.slot-cfw.fyi) does it for you.

## Cart shells

Carts come in colors, like the real thing. slot knows the right color for a lot of games already. To pick your own, add a line to `Labels/cart_shell.txt`:

```
Metroid Fusion = auto 6a3fa0 solid
```

That's the game's file name, then three words:

1. **Shape**: `auto`, `notched` or `rounded`. Only matters for Game Boy carts. `auto` uses the notched cart for original Game Boy games and the rounded one for Color-only games.
2. **Color**: six hex digits, no `#`.
3. **Finish**: `solid`, `clear` or `glitter`.

Keep it all lowercase. A line slot can't read is skipped.

To put a game back to its built-in color, give it just `auto`.

## Wallpapers

Drop a `.png` into `Wallpapers/` and pick it under Wallpaper in Settings → General. Random cycles through all of them.

Make it your screen size:

| Device | Size |
| --- | --- |
| RG SP, RG34XX, RG34XXSP | 720×480 |
| RG35XXSP, RG35XX Plus, RG35XX-H, RG28XX | 640×480 |

Other sizes are scaled to fill the screen and cropped from the centre. Wallpaper Tint darkens it a bit if your carts get lost on it.

## Bezels

For Game Boy and Game Boy Color, drop a `.png` into `Bezels/GB/` or `Bezels/GBC/` and pick it under Bezel in that system's Screen settings.

The bezel is drawn over the game, so leave a transparent hole where the game goes:

| Device | Bezel size | Game area |
| --- | --- | --- |
| RG SP, RG34XX, RG34XXSP | 720×480 | 480×432 at 120, 24 |
| RG35XXSP, RG35XX Plus, RG35XX-H, RG28XX | 640×480 | 427×384 at 107, 48 |

GBA doesn't take your own bezels.

## Boot logo

Put a `.bmp` at `Config/bootlogo.bmp`. slot hands it to the device when it boots, so you'll see it the next time you power on.

slot copies the file as it is, without resizing it, so make it the same size and format as the logo your device already shows.

Delete it to go back to the slot logo.

## BIOS

slot doesn't need any BIOS files. If you have your own, drop them into `BIOS/` and you'll get the real boot animation when a game starts.

- `gba_bios.bin`
- `gbc_bios.bin`
- `gb_bios.bin`

If you link with mGBA, both devices need the same BIOS setup. See [Linking Up](linking.md).
