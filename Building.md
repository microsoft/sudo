# Building Sudo for Windows

Sudo for Windows is a Rust project. If you're new to Rust, you can get started with the [Rust Book](https://doc.rust-lang.org/book/). You can quickly get started with rust by installing and running `rustup`:

```cmd
winget install --id Rustlang.rustup --source winget
rustup update
```

## Building

Rust is nice and straightforward. You can build sudo for the default architecture with a simple

```
cargo build
```

You may want to specify a specific architecture. To do that, you'll want instead:

```
cargo build --target x86_64-pc-windows-msvc
```

(You can also use `i686-pc-windows-msvc` as the target).

### Running tests

Assuming that you passed a target architecture above:

```
cargo test --target x86_64-pc-windows-msvc
```

We have additional manual tests that you can use to validate sudo in the
`tools\tests.ipynb` notebook.

### Formatting and clippy

```
cargo fmt
cargo clippy
```

If your code passes a `cargo build && cargo test && cargo fmt && cargo clippy`, you're ready to send a PR.

### Notes on building with the Microsoft internal toolchain

When we're building this project internally, we need to use an internally-maintained fork of the rust toolchain. This toolchain needs to be used for all production work at Microsoft so we can stay compliant with Secure Development Lifecycle (SDL) requirements.

**If you're external to Microsoft, this next section doesn't apply to you**. You
can use the standard Rust toolchain.

First, install the internal `msrustup` toolchain to install the right version of
all our Rust tools. You can get it from the https://aka.ms/msrustup-win. After
that installs, then you'll probably also need to run the following:

```
rustup default ms-stable
```

That'll select the ms toolchain as the default. If you ever want to switch back, you can always just run

```
rustup default stable-x86_64-pc-windows-msvc
```

Additionally, we've got a separate fork of our `.cargo/config.toml` we need to use for internal builds. Notably, this includes `-Cehcont_guard` to enable EH Continuation Metadata. It also redirects cargo to use our own package feed for dependencies.

You can manually build with that config with:

```
cargo build --config .cargo\ms-toolchain-config.toml
```

Note, if you run that on the public toolchain, you'll most definitely run into ``error: unknown codegen option: `ehcont_guard` `` when building.

### Notes on updating the cargo feed

For internal reasons, we need to maintain a separate Azure Artifacts cargo feed.
Largely we just pull dependencies through from crates.io into that feed. Hence
the config to replace the default cargo feed with our own.

As a maintainer, if you need to update that feed, You should check out the steps
under ["Cargo" on this
page](https://dev.azure.com/shine-oss/sudo/_artifacts/feed/Sudo_PublicPackages).
TL;DR, do the following:

```cmd
az login
az account get-access-token --query "join(' ', ['Bearer', accessToken])" --output tsv | cargo login --registry Sudo_PublicPackages
```

That'll log you in via the Azure CLI and then log you into the cargo feed. I believe that should allow you to pull packages through to the Azure feed, from the public feed. If it doesn't, maintainers should have the necessary permissions to add packages to the feed.
