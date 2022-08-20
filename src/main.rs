#![feature(try_blocks, once_cell)]

use std::cell::LazyCell;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Stdin, stdin, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::thread::sleep;
use std::time::{Duration, Instant};
use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser)]
    mime: String,

    #[clap(short, long, value_parser, default_value_t = 100000)]
    buffer: usize,

    #[clap(short, long, value_parser, default_value_t = 5)]
    buffer_count: usize,

    #[clap(short, long, value_parser, default_value_t = 68719476736)]
    fakelen: u64,

    #[clap(short, long, value_parser)]
    port: u16


}

fn main() {
    let args: Args = Parser::parse();

    let stdin = stdin();
    let mut stdin_buf = BufReader::new(stdin.lock());
    let mut stdin_buf_pos = 0;
    let mut queue = VecDeque::with_capacity(3);

    let mut socket = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), args.port)).unwrap();
    socket.set_nonblocking(false).unwrap();

    let mut incoming = socket.incoming();

    let first_connect = LazyCell::new(Instant::now);

    'a: for x in incoming {
        println!("con!");
        let mut stream = x.unwrap();
        stream.set_nonblocking(false).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        stream.set_write_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut start: u64 = 0;
        let mut get = false;
        {
            let mut stream_buf = BufReader::new(&mut stream);
            let lines: Vec<_> = stream_buf.lines().take_while(|x| x.as_ref().map_or(false, |x| x.as_str() != "")).map(|x| x.unwrap()).collect();
            get = lines[0].starts_with("GET");
            for x in lines {
                println!("{x}");
                if x.to_lowercase().starts_with("Range:") {
                    start = dbg!(x.split("bytes=").nth(2).unwrap().split("-").nth(1).unwrap().parse().unwrap());
                    break;
                }
            }
        }
        let result: std::io::Result<()> = try {
            writeln!(stream, "HTTP/1.1 206 Partial Content\r")?;
            writeln!(stream, "Content-Type: {}\r", args.mime)?;
            writeln!(stream, "Connection: Keep-Alive\r")?;
            writeln!(stream, "Access-Control-Allow-Origin: *\r")?;
            writeln!(stream, "File-Size: {}\r", args.fakelen)?;
            writeln!(stream, "Accept-Ranges: bytes\r")?;
            writeln!(stream, "Content-Range: bytes {start}-{}/{}\r", args.fakelen - 1, args.fakelen)?;
            writeln!(stream, "Content-Length: {}\r", args.fakelen)?;
            writeln!(stream, "\r")?;
        };
        if let Err(e) = result {
            eprintln!("{:#?}", e);
            continue 'a;
        }
        while get {
            while stdin_buf_pos <= start {
                let mut buffer = vec![0; args.buffer];
                stdin_buf.read_exact(buffer.as_mut_slice()).unwrap();
                stdin_buf_pos += args.buffer as u64;
                queue.push_front(buffer);
            }
            let rewind = stdin_buf_pos - start; //how much we need to go back
            let offset = (args.buffer as u64) - (rewind % (args.buffer as u64));
            let queue_pos = (rewind - offset) / (args.buffer as u64);
            if Instant::now() - *first_connect >= Duration::from_secs(10) {
                queue.truncate((queue_pos as usize) + args.buffer_count + 1);
            }

            if let Some(buffer) = queue.get(queue_pos as usize) {
                match stream.write(&buffer.as_slice()[args.buffer - offset as usize..]) {
                    Ok(wrote) => start += wrote as u64,
                    Err(e) => {
                        eprintln!("{:#?}", e);
                        continue 'a;
                    }
                }
            } else {
                eprintln!("not found in buffer, plz increase the buffer size or buffer count");
                continue 'a;
            }
        }
    }
}
