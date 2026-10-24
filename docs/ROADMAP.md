# Roadmap: from Tetris player to architecture testbed

Status: proposal, waiting for review (2026-10-11).

## Goal

Use the emulator to experiment with computer architectures at the digital level:
components exchange binary data over pins and buses, with timing. Planned
experiments:

1. Add external RAM to the PIC16F876 and write a "simple OS" for it that
   uses internal RAM as an L1 cache.
2. Add a second PIC in front of the RAM and use it as an L2-like cache.

### Out of scope

No electrical simulation: no transistors, voltages, rise times or analog
behaviour. A signal is 0, 1 or undriven (high-Z). Timing is modelled as
delays between digital events, such as "SRAM data valid 55 ns after OE falls".

## Where things stand

Update 2026-10-23: phase 4 is done, with SPI. The external RAM is a
Microchip 23LC1024 (128K x 8, 5 V, DIP-8), bit-banged by the PIC on
`boards/spiram` (no LCD). The part, the wiring and the model are described in
`docs/SPI_RAM.md`. gpasm and gpsim are now installed in Claude's
environment, and all tests pass there, including the gpsim comparisons.

Update 2026-10-19: phases 1-3 are done. Trace with
`cargo run -- --vcd out.vcd [NET_OR_BUS...]` (no names: every net and bus),
quit with Ctrl+A x to flush the file. A Tetris run writes about 1.5 MB per
simulated second with all signals. The LCD setup is at ~1.04-1.06 s. The
sections below describe the board before phases 1-2.

Boards are KiCad schematics (2026-10-19): `cargo run -- BOARD.kicad_sch`
(default `boards/tetris/tetris.kicad_sch`), `--check` prints what was
emulated. A symbol is emulated when it has an `Emu` field, e.g.
`PIC16F876 rom=../../TETRIS.hex fosc=4MHz`, `ST7920`, or
`74C922 pins="1:D0 2:D1 3:D2 4:D3 5:DA" keys="a:3 space:5"`. Everything
else (R, C, crystal, regulator) is drawing only. Pins match by name
(`INT/RB0` -> RB0) or by number through the model's pinout or `pins=`.
Nets called GND*/VSS or +*/VCC/VDD are constant 0/1; NAME0..n is traced as
bus NAME. Needs `kicad-cli` (or pass an exported `.net`).

- The PIC16F876 core runs `TETRIS.hex` correctly. `cargo test` passes,
  according to the user's run on 2026-10-11 (gpsim isn't installed in Claude's
  container, so the gpsim comparison tests have only run on the user's side).
- Components: `PIC16F876`, `ST7920` LCD (with busy flag readback), `Keypad`
  (74C922-style encoder), `23LC1024` SPI SRAM.
- Not emulated: interrupts (RETFIE exists, nothing raises them), TMR0, TMR2,
  WDT, MSSP (SPI/I2C), USART, ADC, CCP.
- History of the Tetris debugging: `DEBUG_NOTES.md`.

### How the board works today

- `Component` (`src/common/component.rs`) has `step() -> u32` (cycles used)
  and GPIO groups identified by index: `receive_input`, `output_pending`,
  `get_gpio_values`.
- `Board` (`src/main.rs`) steps every component once per round. After a step,
  each pending source group is copied straight into the subscribers' groups
  (`subscribe(dst, src, [(src_group, dst_group)])`).
- `Board::run` throttles to wall-clock time through `set_cycle_time`.

Limits that matter for the plans below:

- **No shared time base.** One round = one step of each component, whatever
  their clocks. Two PICs at different speeds, or a RAM with an access time,
  can't be expressed.
- **No delays.** Values arrive within the same round they were written.
- **Point-to-point only.** No shared bus, no notion of who drives a line, no
  high-Z, no detection of two drivers fighting. The ST7920 driving PORTC is a
  hand-built special case.
- **Groups are a `usize` of pins** (`TODO` in `src/common/bus.rs`). Wide
  address buses or many signals don't fit well.

## Hardware constraints that shape the design

- **Harvard architecture.** The PIC fetches instructions only from its 8K-word
  internal flash. External RAM can hold data, not directly executable code.
  An "L1 cache" can therefore mean:
  - **Software-managed data cache** (start here): internal RAM (368 bytes,
    some used by the OS itself) caches lines of external RAM. The OS layer
    handles lookup, fill and write-back. Easy to measure.
  - **Bytecode VM**: an interpreter in flash runs a program stored in
    external RAM, with code cached in internal RAM. Code then really goes
    through the cache.
  - **Self-programming**: the 16F87x can write its own flash
    (`EECON1.EEPGD`), so the OS could load overlays. Slow and wear-limited on
    real hardware; free in the emulator.
- **22 I/O pins**, and the LCD already uses PORTA and PORTC. A 32K×8
  parallel SRAM needs ~26 pins (15 address, 8 data, 2-3 control). Options:
  - Multiplexed address/data with a latch component (74HC573-style).
  - SPI SRAM (23K256-style), bit-banged first, MSSP later.
  - Drop or move the LCD for memory experiments.

## Proposed core design

### 1. Simulated time and a scheduler

- Global time in picoseconds as `u64` (lasts ~213 days of simulated time).
- Each component has a clock period, or none for passive parts like RAM.
- Discrete-event scheduler: a priority queue of (time, component, event).
  A clocked component is woken at its next edge; a passive one only when
  an input it listens to changes, or when a delayed output it scheduled
  matures.
- `Board::run`'s wall-clock throttle compares simulated time with real time
  instead of counting cycles.
- Breakpoints, headless mode and the key queue keep working.

### 2. Nets instead of subscriptions

- A **net** is a named signal with a list of pins attached. A **bus** is an
  ordered set of nets (`A0..A14`, `D0..D7`).
- Each pin has a mode: input, output, or tri-state output (can release the net).
- A net's value: driven 0/1, or `Z` when no one drives it. Two drivers with
  different values give `X` and a warning (bus contention). Pull-ups could be
  a net property later.
- A component writes pins with an optional delay ("this output changes 55 ns
  from now"); the scheduler delivers the change to every listener at that time.
- The board netlist is wiring code in Rust, as today, e.g.
  `board.connect("PORTC.0", "LCD.DB0")`. A text netlist format can come later.
- PIC ports map pins through TRIS: TRIS=1 means the pin is an input and
  releases the net.

### 3. Waveform tracing (VCD)

- Optional dump of chosen nets/buses to a `.vcd` file.
- Viewable in GTKWave by the user; Claude can read it as text.
- Plus counters per experiment (hits, misses, cycles per access).

## Phases

Each phase ends with `cargo test` green and Tetris still playable.

| # | Phase | Done when |
|---|---|---|
| 1 | Scheduler + simulated time | Tetris runs on the scheduler; per-component clocks; tests unchanged |
| 2 | Nets, buses, tri-state | LCD and keypad wired through nets; ST7920 PORTC special case gone; contention test |
| 3 | VCD tracing | A Tetris LCD write visible in GTKWave |
| 4 | SRAM component (+ latch or SPI) | Test program writes and reads back external RAM, with access timing. **Done 2026-10-23:** SPI, 23LC1024 |
| 5 | Interrupts (+ TMR0/TMR2) | Timer interrupt test; gpsim comparison where possible |
| 6 | Memory access routines + software cache on the PIC | Hit/miss/latency numbers for a benchmark |
| 7 | "Simple OS": timer tick, memory API, maybe the bytecode VM | Program runs under the OS using the cache |
| 8 | Second PIC as L2 controller | Two MCUs on one board; L1/L2 numbers compared to L1 only |

Phases 1-3 are the risky refactor. Tetris and the core tests are the safety net.

## Open questions for review

1. Memory interface: latch-multiplexed parallel, or SPI? **SPI first
   (2026-10-23).**
2. Does the LCD stay on the board during memory experiments? **It doesn't
   have to (2026-10-23).**
3. Cache experiments start as a data cache, or go straight for the bytecode VM?
4. PIC firmware for experiments: assembly with gpasm (like Tetris), or a C
   compiler (SDCC can target PIC14, with limits)?
5. Other component types you already have in mind (UART link, other MCUs,
   discrete logic like counters/decoders)?

## Tooling for Claude's environment

Claude works inside a Docker container. Nothing here requires running outside
it; it's easier and safer to install these in the image:

- **Required:** `gputils` (`gpasm`, to assemble test programs) and `gpsim`
  (reference simulator for the comparison tests). Installed as of 2026-10-23.
- **Useful:** `sdcc` if we go with C firmware (question 4).
- **Not needed:** GTKWave or any GUI. VCD files are text.

For running outside the container (sandbox, allow/deny rules, per-project
settings): see `docs/claude-setup.md`.
