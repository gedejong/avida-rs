# Install and build

Avida is a Rust workspace. You build it from source with Cargo, Rust's build tool. Building takes a few minutes on first use.

## Install Rust

1. Open [rustup.rs](https://rustup.rs/) and follow the instructions for your system. This installs `rustc` and `cargo`.
2. Open a new shell and check the installation with `cargo --version`. You should see a version line.

A stable toolchain is enough. The workspace forbids `unsafe` code and uses the 2021 edition.

On Linux, the desktop application also needs the usual windowing development packages: libxkbcommon, Wayland or X11, and GTK 3. The command-line program and the terminal interface need nothing extra.

## Get the source

Clone the repository and change into the Rust workspace:

```sh
git clone <repository-url> avida
cd avida/rust/avida-native
```

The documentation submodule `documentation/` (the old wiki) is not needed to build or run anything.

## Build

The workspace has three programs, each its own package. Build the ones you want in release mode, which is many times faster than a debug build:

```sh
cargo build --release -p avida        # the command-line program
cargo build --release -p avida-tui    # the terminal interface
cargo build --release -p avida-gui    # the desktop application
```

After each build, you should see `Finished release` and a binary in `rust/avida-native/target/release/`: `avida`, `avida-tui` and `avida-gui`.

To run them without typing the full path, either add that folder to your `PATH` or copy the binaries somewhere that is already on it.

## Wrap the desktop application on macOS

macOS starts a program that was launched from a bare shell outside the logged-in desktop session (for instance inside `tmux`, or over `ssh`) in the background, so its window can open behind the others. A small script wraps the binary in an application bundle that avoids this:

```sh
crates/gui/scripts/make-app-bundle.sh /tmp/avida-app
open /tmp/avida-app/Avida.app --args "$PWD/my-run" -s 7
```

Everything after `--args` goes to `avida-gui` unchanged. Give the experiment folder as an absolute path, because the bundle starts in `/`.

## Check the build

Run the program's help:

```sh
target/release/avida -h
```

You should see a banner and a list of options. The options are described in [the command-line page](../front-ends/cli.md).

To run the test suite (this takes a while, and it is not needed to use the program):

```sh
cargo test --workspace
```

The full validation procedure that contributors follow is in [Validation gates](../contributing/validation.md).

## Next step

[Your first run](first-run.md).
