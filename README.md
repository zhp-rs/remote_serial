## remote uart
[![Crates.io][crates-badge]][crates-url]
[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]
![Size](https://img.shields.io/github/repo-size/zhp-rs/remote_serial)
![Crates.io](https://img.shields.io/crates/d/remote_serial)
![GitHub release (latest by date)](https://img.shields.io/github/v/release/zhp-rs/remote_serial)
![GitHub tag (latest by date)](https://img.shields.io/github/v/tag/zhp-rs/remote_serial)

[crates-badge]: https://img.shields.io/crates/v/remote_serial.svg
[crates-url]: https://crates.io/crates/remote_serial
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/zhp-rs/remote_serial/blob/master/LICENSE
[actions-badge]: https://github.com/zhp-rs/remote_serial/actions/workflows/main.yml/badge.svg
[actions-url]: https://github.com/zhp-rs/remote_serial

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
help
```bash
$ remote_serial -h
remote_serial 1.0.8

USAGE:
    remote_serial [FLAGS] [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -t, --trace      Trun on trace
    -V, --version    Prints version information

OPTIONS:
    -b, --baud <baud>            Baud rate to use [default: 115200]
    -d, --device <device>        Filter based on name of device
    -o, --output <output>        save serial output to a file
    -c, --password <password>    server password [default: 32485967]
    -p, --port <port>            current server port [default: 6258]
    -s, --server <server>        remote server ip:port
```

server
```bash
remote_serial -d /dev/ttyACM0 # for unix
remote_serial -d COM6 # for windows
```

client
```bash
remote_serial -s ip:port
```
