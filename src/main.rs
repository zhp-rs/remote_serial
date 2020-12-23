use crossterm:: {
    event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::{collections::HashMap, io::Write};
use futures::{future::FutureExt, select, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::AsyncWriteExt;
use tokio_serial::{Serial, DataBits, StopBits, Parity, FlowControl};
use tokio_util::codec::BytesCodec;
use structopt::StructOpt;
use bytes::Bytes;
mod string_decoder;
use string_decoder::StringDecoder;
mod error;
use error::{Result, ProgramError};

#[derive(StructOpt, Debug)]
#[structopt(name="remote_serial")]
struct Opt {
    /// Trun on debugging
    #[structopt(short, long)]
    debug: bool,

    /// Filter based on name of port
    #[structopt(short, long)]
    port: Option<String>,
    
    /// Baud rate to use.
    #[structopt(short, long, default_value = "115200")]
    baud: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let result = real_main().await;
    match result {
        Ok(()) => std::process::exit(0),
        Err(ProgramError::NoPortFound) => {
            writeln!(&mut std::io::stderr(), "No USB serial ports found")?;
            std::process::exit(1);
        }
        Err(err) => {
            writeln!(&mut std::io::stderr(), "Error: {:?}", err)?;
            std::process::exit(2);
        },
    }
}

async fn real_main() -> Result<()> {
    let opt = Opt::from_args();
    let mut settings = tokio_serial::SerialPortSettings::default();

    settings.baud_rate = opt.baud;
    settings.data_bits = DataBits::Eight;
    settings.parity = Parity::None;
    settings.stop_bits = StopBits::One;
    settings.flow_control = FlowControl::None;

    let port_name = match &opt.port {
        Some(port) => port.clone(),
        None => "/dev/ttyUSB0".to_string(),
    };
    let err_port = port_name.clone();
    let mut port = tokio_serial::Serial::from_path(port_name.clone(), &settings)
        .map_err(|e| ProgramError::UnableToOpen(err_port, e))?;
    #[cfg(unix)]
    port.set_exclusive(true)?;
    println!("Connected to {}", port_name);
    enable_raw_mode()?;
    let result = monitor(&mut port, &opt).await;
    disable_raw_mode()?;
    result
}

async fn monitor(port: &mut Serial, opt: &Opt) -> Result<()> {

    let mut reader = EventStream::new();
    let (rx_port, tx_port) = tokio::io::split(port);

    let mut serial_reader = tokio_util::codec::FramedRead::new(rx_port, StringDecoder::new());
    let serial_sink = tokio_util::codec::FramedWrite::new(tx_port, BytesCodec::new());
    let (serial_writer, serial_consumer) = futures::channel::mpsc::unbounded::<Bytes>();

    let exit_code = Event::Key(KeyEvent {
        code: KeyCode::Char('x'),
        modifiers: KeyModifiers::CONTROL,
    });
    let mut client_num : usize = 0;
    let mut clients:HashMap<usize, TcpStream> = HashMap::new();
    let mut poll_send = serial_consumer.map(Ok).forward(serial_sink);
    let mut listener = TcpListener::bind("0.0.0.0:12345").await?;
      loop{
        select! {
            _ = poll_send => {}
            event = reader.next().fuse() => {
                match event {
                    Some(Ok(event)) => {
                        if event == exit_code {
                            break;
                        }
                        if let Event::Key(key_event) = event {
                            if let Some(key) = handle_key_event(key_event, opt)? {
                                serial_writer.unbounded_send(key).unwrap();
                            }
                        } else {
                            println!("Unrecognized Event::{:?}\r", event);
                        }
                    }
                    Some(Err(e)) => println!("crossterm Error: {:?}\r", e),
                    None => {
                        println!("maybe_event returned None\r");
                    },
                }
            },
            serial = serial_reader.next().fuse() => {
                match serial {
                    Some(Ok(serial_event)) => {
                        if opt.debug {
                            println!("Serial Event:{:?}\r", serial_event);
                        } else {
                            print!("{}", serial_event);
                            std::io::stdout().flush()?;
                        }
                    },
                    Some(Err(e)) => {
                        println!("Serial Error: {:?}\r", e);
                        // This most likely means that the serial port has been unplugged.
                        break;
                    },
                    None => {
                        println!("maybe_serial returned None\r");
                    },
                }
            },
            client = listener.next().fuse() => {
                match client{
                    Some(Ok(mut client)) => {
                        println!("client: {:?}\r", client.peer_addr().unwrap());
                        client.write_all(b"hello").await?;
                        client_num += 1;
                        clients.insert(client_num, client);
                    },
                    Some(Err(e)) => {
                        println!("tcp accept failed: {}\r", e);
                        break;
                    },
                    None => {
                        println!("client returned None\r");
                    }
                }
            },

        };
    };
    Ok(())
}

fn handle_key_event(key_event: KeyEvent, opt: &Opt) -> Result<Option<Bytes>> {
    if opt.debug {
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

