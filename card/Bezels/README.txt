# Bezels

Add .png files to GBA, GB or GBC. Pick one in Settings > Screen > Game Boy
Advance (or Game Boy, or Game Boy Color) > Bezel.

Make it the size of the screen with a transparent hole where the game
shows. Where the game shows depends on the Scale. Templates for each screen
and Scale are at https://slot-cfw.fyi/guide/customizing/#bezels

GBA bezels show only on the 640 x 480 devices. On the 720 x 480 devices a
GBA game fills the screen.

A bezel shows in every Scale. To make one for a single Scale, end its name
with _spscreen, _integer or _fit, like Shell_fit.png. It only shows under
Bezel while that Scale is picked. Give each version the same name and
changing Scale switches between them.

If your bezel is solid from just under the game to the bottom edge, like
the AGS-102 bezel, slot moves the game to the top of the screen.

Other sizes are scaled to fill and cropped from the centre. On the 640 x 480
devices a 640 x 427 bezel covers only the game area, with black above and below.
