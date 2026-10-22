# Your SD Card

slot keeps everything on the card in plain folders, so you can poke at it from your computer.

If you use two cards, all of this is on the one in `SD2`.

## The folders

| Folder | What's in it |
| --- | --- |
| `Games/GBA`, `Games/GB`, `Games/GBC` | Your games |
| `Labels/GBA`, `Labels/GB`, `Labels/GBC` | Cart labels |
| `Saves/` | Battery saves, the game's own saves |
| `States/` | Save states |
| `BIOS/` | Optional BIOS files |
| `Wallpapers/` | Your wallpapers |
| `Bezels/GB`, `Bezels/GBC` | Your Game Boy bezels |
| `Config/` | Your settings |
| `Logs/` | Logs, for when something breaks |
| `System/` | slot itself |

slot makes any of these that are missing when it boots.

## Games

| Folder | Takes |
| --- | --- |
| `Games/GBA` | `.gba` |
| `Games/GB` | `.gb`, `.gbc` |
| `Games/GBC` | `.gb`, `.gbc` |

The folder decides which shelf a game goes on.

!!! warning "ROMs must be unzipped!"

    slot does not read `.zip` or `.7z` files.

Put games right in the folder. slot doesn't look in folders inside `Games/GBA` and the rest.

## Names matter

Everything about a game hangs off its file name: its battery save, save states, label, cart shell and emulator choice.

If you rename `Metroid Fusion.gba`, rename its save in `Saves/GBA` too, or the game won't find it.

## Saves and states

- Battery saves are `Saves/<system>/<game>.sav`. slot will also read a `.srm` if that's what you have.
- Save states are in `States/<system>/<emulator>/<game>/`. Each one has a `.png` next to it for the switcher.

Back up `Saves/` and `States/` and you've backed up your progress.

## Config

Most of what's in `Config/` is written by slot when you change a setting. You can open these files in any text editor, but make your edits with slot off, or slot will write over them.

| File | What it holds |
| --- | --- |
| `slot.state` | Your settings and where you left off |
| `controls.txt`, `shortcuts.txt`, `carousel.txt` | Your button layout |
| `favorites.txt`, `recents.txt` | Your Favorites and Recents |
| `selected_core.txt` | Which emulator each GBA game uses |
| `video_mode.txt` | Which Game Boy games you've stretched |
| `cart_shell.txt` | Cart colors, see [Making It Yours](customizing.md) |
| `bootlogo.bmp` | Your boot logo, if you've made one |

If you ever want slot back to how it came, delete `Config/`. You'll get the clock screen again on the next boot.

## Updating

Updates only replace `System/`. Everything else on this list is yours and stays put. See [Updating](../updating.md).
