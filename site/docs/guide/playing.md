# Playing Games

## Inserting a cart

Tap <kbd>A</kbd> on a cart to push it into the slot. If you've played it before, you pick up right where you left off.

Hold <kbd>A</kbd> to start the game fresh instead. The game boots from the beginning, but your battery save is still there, so your in-game save files are safe.

!!! warning "Holding A doesn't delete anything"

    It skips the resume state for this just this launch.

If you'd rather always start fresh, turn off Load State on Insert in Settings → Gameplay.

## Ejecting

Hold <kbd>MENU</kbd> to eject the cart and go back to the shelf. slot creates a save state on the way out, unless you've turned off Save State on Eject in Settings → Gameplay.

## The in-game menu

Tap <kbd>MENU</kbd> while playing. The game pauses behind it.

| Row | Does |
| --- | --- |
| Resume | Back to the game |
| States | Opens the [save state switcher](save-states.md) |
| Settings | Global Screen, Gameplay and Controls |
| Exit Game | Ejects the cart |

## Picking an emulator

GBA games can run on either [mGBA](https://mgba.io) or [gpSP](https://github.com/libretro/gpsp). Game Boy and Game Boy Color games always run on mGBA.

On the shelf, press <kbd>START</kbd> on a GBA cart. It opens to show the circuit board with a chip in one of two sockets. <kbd>Left</kbd> puts it on mGBA, <kbd>Right</kbd> on gpSP. <kbd>A</kbd> saves your choice, <kbd>B</kbd> backs out.

slot remembers the choice per game. mGBA is the default.

Which should you use?

- mGBA is the accurate one. Start here.
- gpSP is faster. It's also the one that does the wireless adapter for Pokémon trading. See [Linking Up](linking.md).

!!! warning "Save states belong to a core"

    States made on mGBA don't show up on gpSP and the other way around. Your battery save is shared, so in-game saves carry over.

## Battery saves

Games that save on the cart write to `Saves/`.

## Rumble

Games with a rumble pak rumble. Turn it off in Settings → Gameplay if you'd rather they didn't.

Pokémon Pinball: Ruby & Sapphire still needs rumble turned on in the game's own options.
