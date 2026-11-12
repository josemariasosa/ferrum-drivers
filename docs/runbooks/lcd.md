cargo +esp check \
  --target xtensa-esp32s3-none-elf \
  --features esp32s3 \
  --lib
  --release

cargo +esp run \
  --release \
  --target xtensa-esp32s3-none-elf \
  --features esp32s3,hd44780 \
  --example esp32s3_hd44780