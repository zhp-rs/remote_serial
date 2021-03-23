## remote uart ![Master](https://github.com/zhp-rs/remote_serial/actions/workflows/rust.yml/badge.svg)

### Install
```bash
cargo install --git https://github.com/zhp-rs/remote_serial
```

### build with xargo
```bash
cargo install xargo
xargo +nightly build --target x86_64-unknown-linux-gnu --release
```

### run

server
```bash
remote_serial -d /dev/ttyACM0 # for unix
remote_serial -d COM6 # for windows
```

client
```bash
remote_serial -s ip:port
```
