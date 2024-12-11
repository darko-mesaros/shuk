# Compilation instructions

To create a statically linked binary, you'll need different approaches depending on the target platform. Here are the methods for each OS:

For Linux (using musl):
```bash
# Install musl target
rustup target add x86_64-unknown-linux-musl

# Build
cargo build --target x86_64-unknown-linux-musl --release
```

For macOS (static linking is not fully possible due to OS restrictions):
```bash
# The best we can do on macOS is use the native target
cargo build --release
```

For Windows:
```bash
# Install the MSVC target
rustup target add x86_64-pc-windows-msvc

# Build
cargo build --target x86_64-pc-windows-msvc --release
```

To make this easier, you can create a `.cargo/config.toml` file in your project:

```toml
[target.x86_64-unknown-linux-musl]
rustflags = ["-C", "target-feature=+crt-static"]

[target.x86_64-pc-windows-msvc]
rustflags = ["-C", "target-feature=+crt-static"]

[target.x86_64-apple-darwin]
rustflags = ["-C", "target-feature=+crt-static"]
```

For Linux, you might also need to install some dependencies:
```bash
# Ubuntu/Debian
sudo apt install musl-tools

# Fedora
sudo dnf install musl-gcc
```

To verify that your binary is statically linked on Linux:
```bash
ldd target/x86_64-unknown-linux-musl/release/your_binary
# Should output "not a dynamic executable"
```

Remember that since we're using system commands (`xclip`, `pbcopy`, `clip`), the binary itself will be static but will still require these system utilities to be present on the target system.
