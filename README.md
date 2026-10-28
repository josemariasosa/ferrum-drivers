picotool uf2 convert target/thumbv8m.main-none-eabihf/release/examples/pico2_dht11 -t elf firmware/pico2_dht11.uf2

elf2uf2-rs \
  target/thumbv8m.main-none-eabihf/release/examples/pico2_dht11 \
  pico2_dht11.uf2

cargo build -p ferrum-dht11 \
  --example pico2_dht11 \
  --features rp2350 \
  --target thumbv8m.main-none-eabihf \
  --release