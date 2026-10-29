# AGENTS.md — Ferrum Drivers

Instructions for coding agents working on Ferrum Drivers.

Ferrum Drivers is an experimental collection of portable `no_std` embedded Rust drivers. Prioritize correctness, simplicity, portability, and real hardware validation.

## Architecture

The repository contains three independent Cargo workspaces:

- **Root (`crates/`)** — Hardware-independent protocol drivers.
- **`platforms/rp2350/`** — RP2350 hardware integrations.
- **`platforms/esp32/`** — Espressif hardware integrations.

Do not merge these workspaces or introduce MCU-specific dependencies into core drivers.

See [ADR-001](docs/adr/001-platform-oriented-workspaces.md) for architectural decisions.

## Development rules

- Prefer small, focused changes over broad refactors.
- Preserve public APIs, protocol timing, and existing behavior unless explicitly required.
- Use `embedded-hal` traits and avoid unnecessary dependencies or abstractions.
- Write deterministic host-side tests for portable protocol logic.
- Treat hardware builds and physical validation as separate verification steps.
- Never claim hardware validation based solely on successful compilation.
- Keep documentation consistent with implementation.
- Do not commit, push, flash hardware, or publish crates without explicit authorization.

## Verification

Run relevant checks from the correct workspace.

- **Core:** `cargo fmt --all -- --check` and `cargo test --workspace`
- **RP2350:** build and check using `thumbv8m.main-none-eabihf`
- **ESP32-C3:** build and check using `riscv32imc-unknown-none-elf --release`

Full commands, firmware flashing, and troubleshooting:

[Development Runbook](docs/runbooks/development.md)

## Known limitations

- The current DHT11 async API performs blocking pulse polling.
- Protocol timing changes require tests and hardware revalidation.
- Supported hardware is limited to documented, verified configurations.

See [DHT11 Audit](docs/audits/ferrum-dht11.md) for outstanding findings.

## Working conventions

1. Inspect relevant code and documentation before editing.
2. Make the smallest coherent change.
3. Add or update tests when behavior changes.
4. Run applicable verification commands.
5. Report changes, test results, and remaining limitations.

Do not modify unrelated files or overwrite existing user changes.
