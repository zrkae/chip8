# chip8 emulator
This is a pretty basic [CHIP-8](https://en.wikipedia.org/wiki/CHIP-8) interpreter/emulator. It uses rust and [SDL2](https://crates.io/crates/sdl2).
It was implemented using [this document](https://github.com/mattmikolay/chip-8/wiki/Mastering-CHIP%E2%80%908) as the reference.

The interpreter part is done, the only things that are still "TODO":
    - Audio (could be implemented using sdl mixer)
    - Changing colors
    - Turning hard-coded constants into cli options
    - Maybe a nice menu, better pausing etc.

But I'll leave that for a particularly rainy day :p

## Games for the emulator
The best collection of games for the chip8 that I am aware of is [the chip8 archive](https://johnearnest.github.io/chip8Archive/).
This emulator should be capable of running all of them without issues.
