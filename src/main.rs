#![recursion_limit = "256"]
use bytes::Bytes;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use futures::{channel::mpsc, future::FutureExt, select, StreamExt};
use std::{
    collections::HashMap,
    io::{Write, ErrorKind},
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    fs::File,
};
use structopt::StructOpt;
use structopt::clap::AppSettings;
use tokio::{
    io::{split, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    net::{TcpListener, TcpStream},
};
use tokio_serial::{DataBits, FlowControl, Parity, Serial, StopBits};
use tokio_util::codec::{BytesCodec, FramedRead, FramedWrite};

mod error;
use error::{ProgramError, Result};

#[cfg(unix)]
const DEVICE: &'static str = "/dev/ttyACM0";
#[cfg(windows)]
const DEVICE: &'static str = "COM6";

#[derive(StructOpt, Debug)]
#[structopt(name = "remote_serial")]
#[structopt(global_settings = &[AppSettings::ColoredHelp])]
struct Opt {
    /// Trun on trace
    #[structopt(short, long)]
    trace: bool,

    /// Filter based on name of device
    #[structopt(short, long)]
    device: Option<String>,

    /// Baud rate to use.
    #[structopt(short, long, default_value = "115200")]
    baud: u32,

    /// remote server ip:port.
    #[structopt(short, long)]
    server: Option<String>,

    /// current server port.
    #[structopt(short, long, default_value = "6258")]
    port: u16,

    /// save serial output to a file
    #[structopt(short, long, parse(from_os_str))]
    log: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let result = real_main().await;
    match result {
        Ok(()) => std::process::exit(0),
        Err(ProgramError::NoPortFound) => {
            writeln!(&mut std::io::stderr(), "No USB serial devices found")?;
            std::process::exit(1);
        }
        Err(err) => {
            writeln!(&mut std::io::stderr(), "Error: {:?}", err)?;
            std::process::exit(2);
        }
    }
}

async fn real_main() -> Result<()> {
    let opt = Opt::from_args();

    let mut save_file: Vec<File> = Vec::new();
    if let Some(pathbuf) = &opt.log {
        save_file.push(File::create(pathbuf)?);
    }
    match &opt.server {
        Some(server) => {
            enable_raw_mode()?;
            let result = client_send(&server, &opt, &save_file).await;
            disable_raw_mode()?;
            println!();
            result
        },
        None => {
            let mut settings = tokio_serial::SerialPortSettings::default();
            settings.baud_rate = opt.baud;
            settings.data_bits = DataBits::Eight;
            settings.parity = Parity::None;
            settings.stop_bits = StopBits::One;
            settings.flow_control = FlowControl::None;

            let device_name = match &opt.device {
                Some(device) => device.clone(),
                None => DEVICE.to_string(),
            };
            let err_device = device_name.clone();
            let mut device = tokio_serial::Serial::from_path(device_name.clone(), &settings)
                .map_err(|e| ProgramError::UnableToOpen(err_device, e))?;
            #[cfg(unix)]
            device.set_exclusive(true)?;
            println!("Connected to {}", device_name);
            enable_raw_mode()?;
            let result = monitor(&mut device, &opt, &save_file).await;
            disable_raw_mode()?;
            println!();
            result
        }
    }
}

async fn client_send(server: &String, opt: &Opt, save_file: &Vec<File>) -> Result<()> {
    let mut reader = EventStream::new();
    let stream = TcpStream::connect(server).await?;
    let (mut receiver, mut sender) = split(stream);
    let exit_code = Event::Key(KeyEvent {
        code: KeyCode::Char('x'),
        modifiers: KeyModifiers::CONTROL,
    });
    loop {
        let mut buffer = [0u8; 128];
        select! {
            event = reader.next().fuse() => {
                match event {
                    Some(Ok(event)) => {
                        if event == exit_code {
                            break;
                        }
                        if let Event::Key(key_event) = event {
                            if let Some(key) = handle_key_event(key_event, opt)? {
                                sender.write_all(&key[..]).await?;
                            }
                        } else if let Event::Resize(_, _) = event {
                            // skip resize event
                        } else {
                            println!("Unrecognized Event::{:?}\r", event);
                        }
                    }
                    Some(Err(e)) => println!("\r\ncrossterm Error: {:?}\r", e),
                    None => {
                        println!("\r\nmaybe_event returned None\r");
                    },
                }
            },
            result = receiver.read(&mut buffer).fuse() => {
                match result {
                    Ok(len) => {
                        if len == 0 {
                            print!("\r\nexit due to server is stoped");
                            break;
                        } else {
                            print!("{}", std::string::String::from_utf8_lossy(&buffer[0..len]));
                            std::io::stdout().flush()?;
                            save_file.iter().for_each(|mut file| file.write_all(&buffer[..]).unwrap());
                        }
                    },
                    Err(err) => {
                        if err.kind() == ErrorKind::ConnectionReset {
                            print!("\r\nexit due to server is stoped");
                        } else {
                            print!("\r\n{:?}", err);
                        }
                        break;
                    }
                }
            },
        };
    }
    Ok(())
}

async fn monitor(device: &mut Serial, opt: &Opt, save_file: &Vec<File>) -> Result<()> {
    let mut reader = EventStream::new();
    let (rx_device, tx_device) = split(device);

    let mut serial_reader = FramedRead::new(rx_device, BytesCodec::new());
    let serial_sink = FramedWrite::new(tx_device, BytesCodec::new());
    let (serial_writer, serial_consumer) = mpsc::unbounded::<Bytes>();

    let exit_code = Event::Key(KeyEvent {
        code: KeyCode::Char('x'),
        modifiers: KeyModifiers::CONTROL,
    });
    let list_code = Event::Key(KeyEvent {
        code: KeyCode::Char('y'),
        modifiers: KeyModifiers::CONTROL,
    });
    let mut writers: HashMap<SocketAddr, WriteHalf<TcpStream>> = HashMap::new();
    let (sender, mut receiver) = mpsc::unbounded::<(SocketAddr, Option<Bytes>)>();
    let mut poll_send = serial_consumer.map(Ok).forward(serial_sink);
    let mut listener = TcpListener::bind((Ipv4Addr::new(0, 0, 0, 0), opt.port)).await?;
    println!("server {:?} is running\r", listener.local_addr().unwrap());
    loop {
        select! {
            _ = poll_send => {},
            event = reader.next().fuse() => {
                match event {
                    Some(Ok(event)) => {
                        if event == exit_code {
                            break;
                        } else if event == list_code {
                            match writers.len() {
                                0 => println!("\r\nNo connected!\r"),
                                1 => {
                                    println!("\r\nThe client is connected:\r");
                                    println!("\taddress: {:?}\r", writers.keys().next().unwrap());
                                },
                                n => {
                                    println!("\r\nThese {} clients are connected:\r", n);
                                    writers.keys().for_each(|x| println!("\taddress: {:?}\r", x));
                                }
                            }
                            continue;
                        }
                        if let Event::Key(key_event) = event {
                            if let Some(key) = handle_key_event(key_event, opt)? {
                                serial_writer.unbounded_send(key).unwrap();
                            }
                        } else if let Event::Resize(_, _) = event {
                            // skip resize event
                        } else {
                            println!("\r\nUnrecognized Event::{:?}\r", event);
                        }
                    }
                    Some(Err(e)) => println!("\r\ncrossterm Error: {:?}\r", e),
                    None => {
                        println!("\r\nmaybe_event returned None\r");
                    },
                }
            },
            serial = serial_reader.next().fuse() => {
                match serial {
                    Some(Ok(serial_event)) => {
                        if opt.trace {
                            println!("Serial Event:{:?}\r", serial_event);
                        } else {
                            print!("{}", std::string::String::from_utf8_lossy(&serial_event[..]));
                            std::io::stdout().flush()?;
                            save_file.iter().for_each(|mut file| file.write_all(&serial_event[..]).unwrap());
                            for (_, writer) in writers.iter_mut() {
                                if let Err(e) = writer.write_all(&serial_event[..]).await {
                                    println!("Send: {:?}\r", e);
                                }
                            }
                        }
                    },
                    Some(Err(e)) => {
                        if e.kind() == ErrorKind::TimedOut {
                            print!("\r\nTimeout: the serial device has been unplugged!");
                        } else {
                            println!("\r\nSerial Error: {:?}\r", e);
                        }
                        break;
                    },
                    None => {
                        println!("\r\nserial returned None\r");
                        break;
                    },
                }
            },
            client = listener.next().fuse() => {
                match client {
                    Some(Ok(client)) => {
                        let addr = client.peer_addr().unwrap();
                        println!("connect from {:?}\r", &addr);
                        let (read, write) = split(client);
                        writers.insert(addr.clone(), write);
                        tokio::spawn(read_stream(addr, read, sender.clone()));
                    },
                    Some(Err(e)) => {
                        println!("tcp accept failed: {}\r", e);
                        break;
                    },
                    None => {
                        println!("\r\nclient returned None\r");
                    }
                }
            },
            client = receiver.next().fuse() => {
                let (addr, buf) = client.unwrap();
                match buf {
                    Some(data) => {
                        if opt.trace {
                            println!("\r\n{:?}\r", &data);
                        }
                        serial_writer.unbounded_send(data).unwrap();
                    },
                    None => {
                        println!("\r\nconneced {:?} is closed!\r", addr);
                        writers.remove(&addr);
                    }
                }
            },

        };
    }
    Ok(())
}

async fn read_stream(
    addr: SocketAddr,
    mut reader: ReadHalf<TcpStream>,
    sender: mpsc::UnboundedSender<(SocketAddr, Option<Bytes>)>,
) {
    loop {
        let mut buffer = [0u8; 128];
        select! {
            result = reader.read(&mut buffer).fuse() => {
                match result {
                    Ok(len) => {
                        if len == 0 {
                            sender.unbounded_send((addr, None)).expect("Channel error");
                            return;
                        } else {
                            sender.unbounded_send((addr, Some(Bytes::copy_from_slice(&buffer[0..len]))))
                            .expect("Channel error");
                        }
                    },
                    Err(_err) => {
                        sender.unbounded_send((addr, None)).expect("Channel error");
                        return;
                    }
                }
            },
        }
    }
}

fn handle_key_event(key_event: KeyEvent, opt: &Opt) -> Result<Option<Bytes>> {
    if opt.trace {
        println!("Event::{:?}\r", key_event);
    }
    let mut buf = [0; 4];

    let key_str: Option<&[u8]> = match key_event.code {
        KeyCode::Backspace => Some(b"\x08"),
        KeyCode::Enter => Some(b"\x0D"),
        KeyCode::Left => Some(b"\x1b[D"),
        KeyCode::Right => Some(b"\x1b[C"),
        KeyCode::Home => Some(b"\x1b[H"),
        KeyCode::End => Some(b"\x1b[F"),
        KeyCode::Up => Some(b"\x1b[A"),
        KeyCode::Down => Some(b"\x1b[B"),
        KeyCode::Tab => Some(b"\x09"),
        KeyCode::Delete => Some(b"\x1b[3~"),
        KeyCode::Insert => Some(b"\x1b[2~"),
        KeyCode::Esc => Some(b"\x1b"),
        KeyCode::Char(ch) => {
            if key_event.modifiers & KeyModifiers::CONTROL == KeyModifiers::CONTROL {
                buf[0] = ch as u8;
                if (ch >= 'a' && ch <= 'z') || (ch == ' ') {
                    buf[0] &= 0x1f;
                    Some(&buf[0..1])
                } else if ch >= '4' && ch <= '7' {
                    // crossterm returns Control-4 thru 7 for \x1c thru \x1f
                    buf[0] = (buf[0] + 8) & 0x1f;
                    Some(&buf[0..1])
                } else {
                    Some(ch.encode_utf8(&mut buf).as_bytes())
                }
            } else {
                Some(ch.encode_utf8(&mut buf).as_bytes())
            }
        }
        _ => None,
    };
    if let Some(key_str) = key_str {
        Ok(Some(Bytes::copy_from_slice(key_str)))
    } else {
        Ok(None)
    }
}
