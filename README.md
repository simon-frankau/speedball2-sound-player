# Speedball II Sound Player

I have recently been reversing Speedball II for the Amiga
(https://github.com/simon-frankau/speedball2-re-amiga), and, inspired
by https://github.com/q3k/track I thought I'd write a little
stand-alone player for the sounds in Speedball II. After all, I've
build tools to extract the graphics from the game, so why not the
sounds?

## To run

From the top-level directory, run `cargo run -- game` to load all the
in-game effects. To hear the title music, run `cargo run -- intro` and
listen to sound 0x2c.

## The sounds

The only sounds used in intro-mode are:

 * 0x2c: Intro music (sequences 1, 2, 3, 4)
 * 0x2d: Silence (sequences 0x18, 0x18, 0x18, 0x18)
 * 0x36: Teletype noise for printing characters (Sequence 19)
 * 0x37: Teletype noise to spaces (Sequence 20)

All the other sounds are valid in game mode.

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

## Structure

The Amiga has 4 sound channels. In the terminology I've built, the
game plays Sounds, which are an assignment of Sequences to
Channels. Sequences are byte code sequences that configure a channel
and play sounds on them. The particular noise produced depends on the
Instrument assigned to a channel. An Instrument will play a Sample in
a particular way. It may have tremolo or vibrato Effects applied to
it, or an ADSR Envelope applied. While the Amiga code supports
Envelopes, they're not used by any of the Sequences here, so I've not
implemented them.

This interface allows you to play Sounds, Sequences and Instruments.

## Other notes

This code is not defensive. If you feed it bad data, it will try to
read out of range and die. You have been warned!

It is not efficient. This makes me feel pretty bad, but given that in
practice it's not performance-critical, I'm trying to err on the side
of easy-to-read rather than efficient.

Graph-drawing is particularly expensive, since it's being done
dynamically every frame (!) and I can't be bothered to optimise. If
you open too many Instrument fold-ups at once, the sound may start to
break up!

I only implement the features used in the actual sounds (I don't want
to put in unnecessary work to build features that are hard to
test. This means that I'm not implementing ADSR envelopes, or a few of
the more obscure byte codes (most of which are just no-ops!)..

The Amiga implementation is modular/well-encapsulated, has features
not actually used by Speedball 2, and generally looks quite
re-usable. As such, I've taken a look at some of the other Bitmap
Brothers hits to see if my reversing here helps there, and... they use
different mechanisms to play their music. Magic Pockets and Gods use
an embedded mod file, while Xenon 2 uses a music player with different
bytecode. Oh well!
