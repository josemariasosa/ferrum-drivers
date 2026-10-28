picotool uf2 convert target/thumbv8m.main-none-eabihf/release/examples/pico2_dht11 -t elf firmware/pico2_dht11.uf2

elf2uf2-rs \
  target/thumbv8m.main-none-eabihf/release/examples/pico2_dht11 \
  pico2_dht11.uf2

cargo fmt --all
cargo test -p ferrum-dht11

cargo check -p ferrum-dht11 \
  --features rp2350 \
  --target thumbv8m.main-none-eabihf

cargo build -p ferrum-dht11 \
  --example pico2_dht11 \
  --features rp2350 \
  --target thumbv8m.main-none-eabihf \
  --release

- [DHT11 — Temperature & Humidity](docs/dht11.md)

cargo check \
  -p ferrum-dht11-rp2350 \
  --lib \
  --target thumbv8m.main-none-eabihf

```sh
# Desde la raíz:
cargo fmt --all -- --check
cargo test --workspace


# Desde la plataforma:
cd platforms/rp2350

cargo fmt --all -- --check

cargo check \
  --lib \
  --features dht11 \
  --target thumbv8m.main-none-eabihf

cargo build \
  --example pico2_dht11 \
  --features dht11 \
  --target thumbv8m.main-none-eabihf \
  --release
```