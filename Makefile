TARGET = x86_64-pc-windows-gnu
RELEASE_DIR = target/$(TARGET)/release

# Default: cross-compile for Windows
all: build

# Install required toolchain (one-time setup)
setup:
	rustup target add $(TARGET)
	brew list mingw-w64 || brew install mingw-w64

lint:
	cargo fmt
	cargo clippy --target=$(TARGET) --all-targets -- -D warnings

# Cross-compile Windows binary from macOS
build:
	cargo build --target=$(TARGET) --release

# Run tests on host platform (cross-compiled tests can't run on macOS)
test:
	cargo test

clean:
	cargo clean

# Show output binary path
print:
	@echo "binary: $(RELEASE_DIR)/sim-relay.exe"
