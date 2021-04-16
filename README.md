## remote uart
[![main](https://github.com/zhp-rs/remote_serial/actions/workflows/main.yml/badge.svg)](https://github.com/zhp-rs/remote_serial/actions/workflows/main.yml)
### Install
```bash
cargo install remote_serial
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
