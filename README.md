# dm32cps — Linux programming software for the Baofeng DM-32UV

[![Buy Me A Coffee](https://img.shields.io/badge/Buy%20Me%20A%20Coffee-support-FFDD00?style=flat-square&logo=buymeacoffee&logoColor=black)](https://buymeacoffee.com/immakinbacon)

A native Linux replacement for the official Windows CPS of the Baofeng DM-32UV (model string
`DP570UV`), written in Rust with an egui interface.

| Crate | What it is |
|---|---|
| `dm32-proto` | Serial protocol: handshake, V-frames, program mode, 4 KB page read/write |
| `dm32-codeplug` | Memory image, block map, record codecs, `.dm32` / official `.data` files, read/write orchestration |
| `dm32-cli` | `dm32` command-line tool (probe, read, write, diff, convert) |
| `dm32-gui` | `dm32cps` graphical editor |

## Download

Prebuilt Linux x86_64 binaries are attached to each
[release](https://github.com/immakinbacon/cpsdm32/releases/latest):

- `dm32-linux-x86_64` — command-line tool, fully static, runs on any x86_64 Linux
- `dm32cps-linux-x86_64` — graphical editor, needs glibc 2.28 or newer and an X11 or Wayland desktop
- `SHA256SUMS` — checksums (`sha256sum -c SHA256SUMS`)

```sh
chmod +x dm32-linux-x86_64 dm32cps-linux-x86_64
```

## Building

```sh
cargo build --release                       # target/release/dm32 and dm32cps
cargo build --release -p dm32-cli --target x86_64-unknown-linux-musl   # fully static dm32
```

The GUI needs an X11 or Wayland session with OpenGL.

Shippable binaries (what CI produces) come from `tools/build-release.sh`, which writes `dist/`:
`dm32` fully static, and `dm32cps` linked through zig against glibc 2.28 so it runs on older
distributions too.

## Serial port access

The programming cable is a USB serial adapter (`/dev/ttyUSB*`). Your user must be allowed to open it:

```sh
sudo usermod -aG dialout $USER    # then log out and in again
```

## Command line

```sh
dm32 ports                                   # list USB serial ports
dm32 probe                                   # identify the radio (no programming mode)
dm32 read -o radio.dm32                      # read the codeplug
dm32 read -o radio.dm32 --contacts           # ... including the DMR contact database
dm32 write radio.dm32 --dry-run              # show what would be written
dm32 write radio.dm32                        # write changed blocks, back up first, verify
dm32 diff a.dm32 b.dm32                      # compare two codeplugs block by block
dm32 convert radio.data radio.dm32           # official CPS file ↔ native file
```

`--trace FILE` logs every byte exchanged with the radio; please attach it to bug reports.

Notes on the radio:

- The radio answers the handshake once per power-on. If a command reports *no reply*, switch the
  radio off and on again.
- Some firmware (reported for `NRF.01.049`) needs `--rts high --dtr low`.
- There is no "exit programming mode" command; closing the port resets the radio.

## Safety

- Every write first reads the radio and saves its current contents as a backup
  (`dm32-backup-<time>.dm32` for the CLI, `~/.local/share/dm32cps/backups/` for the GUI).
- Only blocks that differ from the radio are written; the calibration block is never written.
- Writes are verified by reading back unless disabled.

## Files

- `.dm32` — this program's lossless format (all codeplug blocks, contact database pages, radio info).
- `.data` — the official CPS save file, readable and writable for interoperability.

## Credits

Protocol knowledge comes from the [DM32-Protocol-Spec](https://github.com/infamy/DM32-Protocol-Spec)
(MIT), [qdmr](https://github.com/hmatuschek/qdmr) (GPL-3), neonplug and the dmrconfig DM-32 fork.
Test captures in `crates/dm32-proto/tests/fixtures` are from the DM32-Protocol-Spec.

## License

GPL-3.0-or-later.
