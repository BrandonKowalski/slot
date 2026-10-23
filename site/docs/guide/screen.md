# The Screen

Each system gets its own screen settings, so you can have a grid on your GBA games and Simpletex on your Game Boy ones. Find them in Settings → Screen, or in the in-game menu while you play.

## Shaders

Shaders make the picture look more like a real handheld screen.

| GBA | Game Boy and Game Boy Color |
| --- | --- |
| Off | Off |
| LCD3x | LCD3x |
| Grid (default) | Grid |
| Dot | Simpletex (default) |

<kbd>SELECT</kbd> + <kbd>A</kbd> and <kbd>SELECT</kbd> + <kbd>B</kbd> step through them while you play.

## Color correction

The original screens were dim and washed out, and developers took this into account while making their games. Color correction pulls the colors back toward what you'd have seen on original hardware. It's off by default.

<kbd>SELECT</kbd> + <kbd>Y</kbd> toggles it in game.

## Game Boy palettes

Original Game Boy games can be shown in other colors. Pick one under Settings → Screen → Game Boy → Palette:

- **Auto** lets mGBA pick, the way a Game Boy Color colors in old games.
- **Grayscale**, **DMG Green**, **GB Pocket** and **GB Light** for the classics.
- The 12 Game Boy Color presets and 32 Super Game Boy palettes.

<kbd>SELECT</kbd> + <kbd>L2</kbd> / <kbd>R2</kbd> step through the same list while you play.

Palettes only apply to original Game Boy games.

## Stretching

Just like the OG, <kbd>L1</kbd> stretches Game Boy and Game Boy Color games across the screen. <kbd>R1</kbd> puts them back. slot remembers your pick per game.

Bezels are hidden while a game is stretched.

## Bezels

A bezel is a frame drawn around the game.

- **Game Boy and Game Boy Color**: None, Default, AGS-102 on 640×480 devices, or your own from `Bezels/`.
- **Game Boy Advance**: only on 640×480 devices, where AGS-102 draws the bottom of a GBA SP under the game.

See [Customization](customizing.md) for details on how to make your own.

## Brightness and blue light

| Button | Does |
| --- | --- |
| <kbd>SELECT</kbd> + <kbd>Up</kbd> / <kbd>Down</kbd> | Brightness |
| <kbd>SELECT</kbd> + <kbd>Left</kbd> / <kbd>Right</kbd> | Blue light |

These work on the shelf and in game. They're also rows in Settings → Screen → General.

Blue light warms the picture up for playing in the dark. At its lowest, the brightness shortcut can turn the backlight all the way off.
