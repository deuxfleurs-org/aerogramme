# Installation

Install a Rust nightly toolchain: [go to Rustup](https://rustup.rs/).

Install and deploy a Garage cluster: [go to Garage documentation](https://garagehq.deuxfleurs.fr/documentation/quick-start/). Make sure that you download a binary that supports K2V. Currently, you will find them in the "Extra build" section of the Download page.

Clone Aerogramme's repository:

```bash
git clone https://git.deuxfleurs.fr/Deuxfleurs/aerogramme/
```

Compile Aerogramme:

```bash
cargo build
```

Check that your compiled binary works:

```bash
cargo run
```

You are now ready to [setup Aerogramme!](./setup.md)
