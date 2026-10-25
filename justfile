# universal-tiling-window-manager dev tasks. `just` lists them.

set windows-shell := ["pwsh", "-NoLogo", "-NoProfile", "-Command"]

config := env_var_or_default("UTWM_CONFIG", "")
config_flag := if config != "" { "--config '" + config + "'" } else { "" }

# List the recipes.
default:
    @just --list

# The CI gate: format, lint, test.
check:
    cargo fmt --all --check
    cargo clippy --workspace --locked -- -D warnings
    cargo test --workspace --locked

# Unit and integration tests.
test:
    cargo test --workspace --locked

# Run the tray app from source, logging every action.
[windows]
run:
    $env:UTWM_LOG = 'debug'; cargo run -p universal-tiling-window-manager -- {{config_flag}}

# Run the menu bar app from source, logging every action.
[macos]
run:
    UTWM_LOG=debug cargo run -p universal-tiling-window-manager -- {{config_flag}}

# Show what the export binds on this machine.
show:
    cargo run -q -p universal-tiling-window-manager -- {{config_flag}} check

# Log every key the keyboard (or RustDesk) delivers. Windows only.
probe seconds="60":
    cargo run -q -p universal-tiling-window-manager -- {{config_flag}} probe {{seconds}}

# End-to-end check against the running app; it opens and moves only its own window.
e2e:
    cargo run -q -p utwm-e2e

# Install the release build into ~/.cargo/bin, stopping a running copy first.
[windows]
install:
    -universal-tiling-window-manager quit
    cargo install --locked --force --path apps/desktop

# Build the app bundle into target/release/bundle/macos.
[macos]
bundle:
    cargo run -q --locked -p utwm-bundle

# Install the app into /Applications and start it, quitting a running copy first.
[macos]
install:
    cargo run -q --locked -p utwm-bundle -- --install

# Supply-chain checks.
deny:
    cargo deny check
