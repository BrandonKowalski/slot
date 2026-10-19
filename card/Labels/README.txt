# Labels

Add a .png to GBA, GB or GBC, named after the ROM. Labels are the same on
every device.

  Games/GBA/Metroid Fusion.gba  ->  Labels/GBA/Metroid Fusion.png

  Cart              Shape    Size
  GBA               1.94:1   1280 x 661
  Game Boy          1.13:1   1024 x 901
  Game Boy Color    1.12:1   1024 x 901

Other sizes are scaled to fill the label and cropped from the centre.
Cart Studio makes these: https://studio.slot-cfw.fyi


# Shells

cart_shell.txt sets a cart's shell, one line per game:

  Metroid Fusion = auto 6a3fa0 solid

  Outline   auto, notched or rounded
  Colour    hex RGB
  Finish    solid, clear or glitter
