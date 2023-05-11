# Speedball II Sound Player

I have recently been reversing Speedball II for the Amiga
(https://github.com/simon-frankau/speedball2-re-amiga), and, inspired
by https://github.com/q3k/track I thought I'd write a little
stand-alone player for the sounds in Speedball II. After all, I've
build tools to extract the graphics from the game, so why not the
sounds?

## TODO


 * Extract the sounds into standalone files separate from the main
   game.
 * Build tooling to extract the sound data into appropriate data
   structures.
 * Start playing sounds
   * Add envelopes
   * Add missing bytecode functionality
   * And then multi-channel sounds
   * wav export may also be nice
   * I've been thinking about serialising user notes with... dunno,
     serde, or something?
   * Sample waveform visualisation might be nice.

## Data

 * `data/main.bin` is basically `overlay_00.bin`, taken from my
   Speedball II Amiga repo. In order to incorporate Overlay #27, I
   concatendated `overlay_27.bin` onto the end of the file, and then
   overwrote offset 0x1a478 from 0x15118 (where the overlay gets
   loaded) to 0x1b000 (where it gets placed after the end of the file.
 * `data/intro` was created with `dd if=unpacked.bin of=intro.bin bs=1
   skip=0x1bbba count=0x2d9fc`. I then overwrite offset 0x29df2 from
   0x1146 to 0x0c64 in order to stop the sample for Instrument 39 from
   reading into data structures/code.
   
TODO: Need the intro music.

TODO: `sound_table`, which glues together the sequences across
multiple channels, is not in this memory range. I will need it later.

## Other notes

This code is not defensive. If you feed it bad data, it will try to
read out of range and die. You have been warned!

It is not efficient. This makes me feel pretty bad, but given that in
practice it's not performance-critical, I'm trying to err on the side
of easy-to-read rather than efficient.
