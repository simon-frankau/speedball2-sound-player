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
   * Fix up the overlay weirdness.
 * Build tooling to extract the sound data into appropriate data
   structures.
 * Slap an egui on it.
 * Start playing sounds
   * Start with playing raw samples
   * Then incrementally add the ability to play sequences
   * And then multi-channel sounds
   * wav export may also be nice
   * I've been thinking about serialising user notes with... dunno,
     serde, or something?
