# mouse-jiggler

Tiny cross-platform mouse jiggler. Two builds in one repo:

- **Universal APE** (`c-cosmo/`) — single ~765 KB binary that runs on Linux + macOS + Windows × x86_64 + arm64 via [Cosmopolitan Libc](https://justine.lol/cosmopolitan/) / [Actually Portable Executable](https://justine.lol/ape.html). One file, six targets.
- **Per-OS Rust** (`src/`) — zero third-party crate dependencies. Cross-compiles to six native binaries; the two Mac slices fuse into one Mach-O universal binary via `lipo`.

## Why two implementations

The "single binary that runs on Linux + macOS + Windows" goal **cannot** be hit in pure Rust today: Rust's `libc`/`std` hard-code platform constants like `EINVAL` at compile time per target, which poisons Cosmopolitan's polyglot runtime on non-Linux hosts. Pure C against Cosmopolitan is the only proven path for one-binary-runs-everywhere. The Rust version is preserved for users who'd rather ship native per-OS binaries.

For the talks and history behind cross-OS / multi-arch fat binaries — Justine Tunney's APE work and Ryan Gordon's failed FatELF kernel patch — see the bottom of this file.

## Usage

```
mouse-jiggler [OPTIONS]

OPTIONS:
  -m, --mode <MODE>          pixel | circle | random   [default: pixel]
  -i, --interval <DURATION>  time between jiggles      [default: 30s]
  -d, --distance <PIXELS>    movement amplitude        [default: 1]
      --max-runtime <DUR>    auto-stop after duration  [default: unlimited]
      --once                 jiggle once and exit
  -q, --quiet                suppress output
  -v, --verbose              per-iteration logging
  -h, --help / -V, --version

DURATION: integer with optional s/m/h suffix (default seconds), e.g. 30s, 5m, 2h
MODES:
  pixel    Move <distance> pixels right then back. Imperceptible at distance=1.
  circle   Trace a small circle of radius <distance> over 8 steps and return.
  random   Random offset in [-distance,+distance]^2, then return to origin.
```

## Prerequisites

You build everything yourself from source. This repo publishes no binary releases and no CI artifacts — only build instructions. Pick whichever build matches your need.

| Build | Host requirement | Tools |
|---|---|---|
| **Universal APE** (one binary, all 6 OS×arch targets) | Linux or macOS host (x86_64 or arm64) | `curl`, `unzip`, `make`, a POSIX shell |
| **Per-OS Rust** (one binary per target) | Any OS that Rust runs on | Rust ≥ 1.94 via [rustup.rs](https://rustup.rs); plus `lipo` if you want a Mac universal binary |

The APE build cannot run on a Windows host (cosmocc is Linux/macOS-only). To produce an APE from a Windows machine, use WSL2 or build inside a Linux Docker container.

Install Rust if you don't have it:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

On macOS you also need the Command Line Tools (provides `lipo`, `make`, etc.):

```sh
xcode-select --install
```

On Debian/Ubuntu, the host needs `curl unzip make build-essential` for the APE build, or just `curl build-essential` for Rust.

## Build A — Universal APE (recommended)

Builds one ~765 KB binary that runs on Linux + macOS + Windows × x86_64 + arm64. Build host must be Linux or macOS.

```sh
# 1. Fetch cosmocc (~440 MB zip, ~1.3 GB extracted)
mkdir -p tools && cd tools
curl -L -o cosmocc.zip https://cosmo.zip/pub/cosmocc/cosmocc.zip
mkdir -p cosmocc && cd cosmocc && unzip -q ../cosmocc.zip
cd ../..

# 2. Build (~3 seconds)
cd c-cosmo
make            # produces ./mouse-jiggler

# 3. Run on the build host
./mouse-jiggler --once -m pixel -d 1 -v

# 4. Copy the binary to any target machine and run it there
#    — same file, no recompilation, no .com/.exe rename needed unless on Windows
```

The output `c-cosmo/mouse-jiggler` is one polyglot file. To deploy:

| Target OS | How to run |
|---|---|
| Linux x86_64 / arm64 | `./mouse-jiggler` (needs `libX11.so.6` + `libXtst.so.6` installed) |
| macOS x86_64 / arm64 | `./mouse-jiggler` (may need to grant Accessibility / Input Monitoring permission) |
| Windows x86_64 / arm64 | rename to `mouse-jiggler.exe`, then double-click or run from `cmd`/PowerShell |
| FreeBSD / OpenBSD / NetBSD x86_64 | `./mouse-jiggler --help` boots; mouse-move not implemented for BSDs |

Internally, the binary header is simultaneously a valid Linux ELF, macOS Mach-O loader stub, Windows PE/COFF, and POSIX shell script. The shell-script bootstrap path runs `uname -m` to pick the correct slice (`x86_64` or `aarch64`) and re-execs the appropriate ELF section. On Windows, the OS reads it as PE32+ directly; on macOS, the embedded Mach-O loader takes over.

## Build B — Per-OS Rust binaries

Native build for the current host:

```sh
cargo build --release            # ./target/release/mouse-jiggler
cargo test  --release            # CLI parser unit tests
```

Cross-compile to all six targets:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin \
                  aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu \
                  aarch64-pc-windows-msvc x86_64-pc-windows-msvc

# Native targets where you have a working linker
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

# Mac universal binary (one Mach-O, both Mac arches) — only works on macOS
mkdir -p dist
lipo -create -output dist/mouse-jiggler-macos-universal \
     target/aarch64-apple-darwin/release/mouse-jiggler \
     target/x86_64-apple-darwin/release/mouse-jiggler
```

### Cross-compilation tips (when not using a native runner)

| From → To | Recommended approach |
|---|---|
| Mac → Linux | `cargo install cross --git https://github.com/cross-rs/cross` then `cross build --release --target x86_64-unknown-linux-gnu` (uses Docker) |
| Mac/Linux → Windows | `cargo install cargo-xwin && cargo xwin build --release --target x86_64-pc-windows-msvc` (downloads MSVC headers/libs) |
| Anywhere → Anywhere | Use the GitHub Actions matrix in `.github/workflows/ci.yml` — every (OS, arch) builds on a native runner. |

## Runtime requirements

| Target | Requirement |
|---|---|
| Linux | `libX11.so.6` + `libXtst.so.6` (Debian/Ubuntu: `libx11-6 libxtst6`; Fedora: `libX11 libXtst`). Wayland: works for X11 clients via XWayland; pure Wayland not supported. |
| macOS | Synthesized `CGEvent`s may need Accessibility or Input Monitoring permission under System Settings → Privacy & Security depending on macOS version. |
| Windows | None. |

## How it works (C/Cosmopolitan)

`c-cosmo/main.c` runs one `main()` that dispatches on the host at startup:

```c
if (IsXnu())     mac_init();   // CoreGraphics + CoreFoundation via cosmo_dlopen → CGEventPost
if (IsLinux())   linux_init(); // libX11.so.6 + libXtst.so.6 via cosmo_dlopen → XTestFakeRelativeMotionEvent
if (IsWindows()) win_init();   // user32.dll → SendInput (declared __msabi for MS x64 ABI)
```

Each branch uses `cosmo_dlopen` (Cosmopolitan's polyglot dlopen — wraps `LoadLibrary` on Windows, `dlopen` on POSIX) to load the platform-native input API at runtime. There is no #ifdef per OS in the source; the same compiled code contains all three paths.

## CI

The PR that brings in this code uses a matrix in `.github/workflows/ci.yml` that builds and tests both implementations on five native runners:

| | Linux | macOS | Windows |
|---|---|---|---|
| **x86_64** | `ubuntu-latest` | _no public runner_ | `windows-latest` |
| **arm64** | `ubuntu-24.04-arm` | `macos-latest` | `windows-11-arm` |

GitHub Actions no longer offers a free public `macos-13` (Intel) runner, so x86_64 macOS is not exercised in CI. The x86_64 macOS slice is still emitted by the APE build and verified manually by the project author. Triggered only on `pull_request` — `main` itself is not exercised by CI after merge.

## Background

Cross-OS / multi-arch fat binaries — talks worth watching:

- **Justine Tunney — "Redbean: Actually Portable Executable Web Server"** (Speakeasy JS, May 2021): https://www.youtube.com/watch?v=1ZTRb-2DZGs — the canonical APE talk, walks through the polyglot trick.
- **Ryan Gordon — "Anatomy of an (alleged) failure"** (SouthEast LinuxFest, 23 June 2010): post-mortem of FatELF, his rejected kernel patch for Linux universal binaries. LWN coverage: https://lwn.net/Articles/392862/.
- Justine's APE format walkthrough: https://justine.lol/ape.html.

## License

MIT — see [LICENSE](LICENSE).
