# External RAM: Microchip 23LC1024

Written 2026-10-23 for phase 4 of `docs/ROADMAP.md`. Board:
`boards/spiram/spiram.kicad_sch`; firmware: `boards/spiram/spiram_test.asm`;
model: `src/hw/sram23lc1024.rs`. Datasheet: Microchip DS20005142C,
<https://ww1.microchip.com/downloads/en/DeviceDoc/20005142C.pdf>.

## Why this part

| | 23LC1024 (chosen) | 23K256 | 23LC512 |
|---|---|---|---|
| Size | 128K x 8 | 32K x 8 | 64K x 8 |
| Supply | 2.5-5.5 V | 2.7-3.6 V | 2.5-5.5 V |
| Address | 24 bits (17 used) | 16 bits | 16 bits |
| Package | DIP-8, SOIC, TSSOP | DIP-8, SOIC, TSSOP | DIP-8, SOIC, TSSOP |
| SPI clock | 20 MHz | 20 MHz | 20 MHz |

- **Runs at 5 V**, like the PIC16F876 board. The 23K256 is the classic
  "SPI SRAM for PICs", but it is 3.3 V only, so it would need level shifting.
- **DIP-8**, so it fits a breadboard or the real Tetris-style board.
- **128 KB** is over 350 times the PIC's 368 bytes of RAM, enough for a cache
  experiment (and later a bytecode VM) to have something to cache.
- The 3-byte address costs 8 more SPI clocks per access than the 23LC512.
  That overhead is realistic, and it shows up in the measurements. The
  23LC512 has the same commands, modes and pinout, so supporting it would
  only change the size and the number of address bytes.

## Wiring on `boards/spiram`

| 23LC1024 pin | Net | PIC16F876 |
|---|---|---|
| 1 CS | `RAM_CS`, 10k pull-up | RC2 |
| 2 SO | `SPI_MISO` | RC4/SDI |
| 5 SI | `SPI_MOSI` | RC5/SDO |
| 6 SCK | `SPI_SCK` | RC3/SCK |
| 3 SIO2, 7 HOLD | +5V | |
| 8 VCC, 4 VSS | +5V, GND | |

- The SPI lines are on the PIC's MSSP pins, so the hardware SPI can replace
  bit-banging later without rewiring.
- The pull-up keeps the RAM deselected while the PIC's pins are still
  inputs after reset.
- The datasheet says not to leave SIO2 floating, and to tie HOLD high when
  it isn't used.
- KiCad has no 23LC1024 symbol. The schematic uses `Memory_EEPROM:25LCxxx`
  with the value set to 23LC1024: the DIP-8 SPI pinout is the same, and on
  that symbol pin 3 is named ~WP.
- The LCD is not on this board; PORTA and PORTB are free.

Emu field: `23LC1024` (no parameters). The pins are matched by number,
following the pinout above.

## What the model does

- **SPI modes 0 and 3.** SI is sampled on the rising edge of SCK, and SO
  changes after the falling edge, so the clock's idle level doesn't matter.
- **Commands:** READ (0x03), WRITE (0x02), RDMR (0x05), WRMR (0x01) and
  RSTIO (0xFF, a no-op in SPI mode). EDIO and EQIO, and any unknown
  command, print a warning; the rest of that transaction is ignored.
- **Modes:** sequential (the power-up default) wraps from 0x1FFFF to 0, page
  mode wraps within the 32-byte page, and byte mode transfers a single byte.
  The upper 7 address bits are ignored. Writing the reserved mode 11 prints a
  warning and keeps the old mode.
- **Timing**, using the datasheet's maximum values (I-temp):
  - SO is valid 25 ns (tV) after SCK falls.
  - SO is released 20 ns (tDIS) after CS rises.
  - SO is released as soon as HOLD falls, and is valid again 50 ns (tHV)
    after HOLD rises.
- **HOLD:** SCK edges are ignored while HOLD is low.
- **Contents** start as all zeros. Real SRAM starts with random contents.

Assumptions and gaps:

- **Byte mode:** the datasheet only says operations are "limited to only one
  byte". Clocks after that byte are ignored, and SO is released.
- **HOLD** acts as soon as it falls. The real chip waits until SCK is low.
- **No checks of the master's timing** (clock speed, setup and hold times).
  A bit-banging PIC at 4 MHz is far too slow to break them; a faster
  master would need them.
- **SDI/SQI modes** are not emulated.

## Measured

`spiram_test.asm` bit-bangs SPI at 11 instruction cycles per bit, which
works out to about 90 µs per byte at 4 MHz, or about 11 KB/s. It runs these
steps in about 9.9 ms of simulated time:

1. Read the mode register (RDMR).
2. Write 32 bytes across the end of the array.
3. Read the 32 bytes back.
4. Switch to byte mode (WRMR) and read address 0.
5. Read the mode register again.

`cargo test spiram` checks the results and that there's no bus contention.
To look at the waveforms:

```
cargo run -- boards/spiram/spiram.kicad_sch --vcd spi.vcd RAM_CS SPI_SCK SPI_MOSI SPI_MISO
```

There is no display, so stop it with Ctrl+C.

Rebuild the firmware after editing it:

```
cd boards/spiram && gpasm -p p16f876 -o spiram_test.hex spiram_test.asm
```
