// server.rs
// TCP server that loads TreeMap from disk, persists after mutations.
use clap::{ArgAction, Parser};
use kvstore::TreeMap;
use std::fs::*;
use std::io::{BufRead, BufReader, Seek, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, RwLock};

struct Log {
    size: u32,
    file: File,
    snapshot_interval: u32,
    buf: String,
    dbfile_name: String,
    map: Arc<RwLock<TreeMap<String, String>>>,
}

impl Log {
    pub fn new(args: Arc<Args>, map: Arc<RwLock<TreeMap<String, String>>>) -> std::io::Result<Self> {
        Ok(Log {
            size: 0,
            file: File::options().append(true).create(true).open(args.logfile.as_str())?,
            snapshot_interval: args.snapshot_interval as u32,
            buf: String::new(),
            dbfile_name: args.dbfile.clone(),
            map,
        })
    }
    pub fn schedule_append(&mut self, line: impl AsRef<str>) {
        self.buf.push_str(line.as_ref());
        self.buf.push_str("\r\n");
        self.size += 1;
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if !self.buf.is_empty() {
            let res = self.file.write_all(self.buf.as_bytes());
            self.buf.clear();
            self.maybe_snapshot().expect("Snapshot failed");
            res
        } else {
            Ok(())
        }
    }
    fn maybe_snapshot(&mut self) -> std::io::Result<()> {
        if self.size >=  self.snapshot_interval {
            self.write_snapshot()?;
        }
        Ok(())
    }
    fn write_snapshot(&mut self) -> std::io::Result<()> {
        self.map.read().unwrap().save_to_file(self.dbfile_name.as_str())?;
        self.file.set_len(0)?;
        self.file.rewind()?;
        self.size = 0;
        Ok(())
    }
}
impl Drop for Log {
    fn drop(&mut self) {
        self.flush().expect("flush failed");
    }
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// IP address and port to bind the server socket to
    #[arg(short, long, default_value = "127.0.0.1:4000")]
    addr: String,

    /// Do not persist the database contents to disk
    #[arg(short, long, default_value = "false")]
    memonly: bool,

    /// Run the server single-threaded
    #[arg(short, long, default_value_t = true, action = ArgAction::Set)]
    singlethread: bool,

    /// Save log to disk every x requests
    #[arg(short, long, default_value_t = 100)]
    snapshot_interval: u16,

    /// Location of the database file
    #[arg(short, long, default_value = "kvstore.db")]
    dbfile: String,

    /// Location of the database transaction log
    #[arg(short, long, default_value = "kvstore.log")]
    logfile: String,

    /// Pass this code to the server EXIT command to have it exit
    #[arg(short, long, default_value = "")]
    exit_code: String,
}

fn handle_client(
    args: Arc<Args>,
    stream: TcpStream,
    map: Arc<RwLock<TreeMap<String, String>>>,
    log: Arc<RwLock<Log>>,
) {
    let mut writer = stream.try_clone().unwrap();
    let reader = BufReader::new(&stream);
    let mut lines = reader.lines();
    let mut response = String::new();
    while let Some(Ok(line)) = lines.next() {
        let parts: Vec<&str> = line.trim_end().splitn(3, ' ').collect();
        match parts[0] {
            "GET" if parts.len() == 2 => {
                let map = map.read().unwrap();
                response.push_str(&match map.get(&parts[1].to_string()) {
                    Some(v) => format!("OK {}\r\n", v),
                    None => "ERR NotFound\r\n".into(),
                });
            }
            "SET" if parts.len() == 3 => {
                map.write().unwrap().insert(parts[1].to_string(), parts[2].to_string());
                response.push_str("OK\r\n");
                log.write().unwrap().schedule_append(&line);
            }
            "REMOVE" if parts.len() == 2 => {
                response.push_str(match map.write().unwrap().remove(&parts[1].to_string()) {
                    Some(_) => {
                        "OK\r\n"
                    }
                    None => "ERR NotFound\r\n",
                });
                log.write().unwrap().schedule_append(&line);
            }
            "SEEK" if parts.len() == 2 => {
                let map = map.read().unwrap();
                response.push_str(&match map.seek_ge(&parts[1].to_string()) {
                    Some((k, v)) => format!("OK {} {}\r\n", k, v),
                    None => "ERR NotFound\r\n".into(),
                });
            }
            "ENDBATCH" => {
                writer.write_all(response.as_bytes()).unwrap();
                response = String::new();
            }
            "EXIT" if parts.len() == 2 && parts[1] == args.exit_code => {
                eprintln!("Received EXIT command with correct exit code. Exiting.");
                std::process::exit(0);
            }
            _ => {
                response = "ERR UnknownCommand\r\n".into();
            } // This is a handy special command to help with profiling the server. Would
              // not recommend having a command like this in your typical key-value store!
        };
    }
    log.write().unwrap().flush().expect("flush failed");
}

fn recover_from_log(map: &mut TreeMap<String, String>, log: File) {
    let mut lines = BufReader::new(log).lines();
    println!("Recovering from log...");
    let mut count = 0;
    while let Some(Ok(line)) = lines.next() {
        count += 1;
        let parts: Vec<&str> = line.trim_end().splitn(3, ' ').collect();
        match parts[0] {
            "SET" if parts.len() == 3 => {
                map.insert(parts[1].to_string(), parts[2].to_string());
            }
            "REMOVE" if parts.len() == 2 => {
                map.remove(&parts[1].to_string());
            }
            _ => {
                panic!("Bad log entry.");
            }
        }
    }
    println!("Recovered {count} updates from log.\n");
}

fn main() -> std::io::Result<()> {
    let args = Arc::new(Args::parse());

    let mut map = match TreeMap::load_from_file(&args.dbfile) {
        Ok(m) => m,
        Err(_) => TreeMap::new(),
    };

    {
        // Create or open pidfile and write PID to it
        let mut file = std::fs::File::create("server_pid.txt").unwrap();
        writeln!(file, "{}", std::process::id())?;
    }

    if let Ok(true) = std::fs::exists(args.logfile.as_str()) {
        recover_from_log(&mut map, File::open(args.logfile.as_str()).unwrap());
    }

    let map = Arc::new(RwLock::new(map));
    let log = Arc::new(RwLock::new(Log::new(args.clone(), map.clone())?));

    let listener = TcpListener::bind(&args.addr)?;
    println!("Server listening on {}", args.addr);

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                // Nagle's algorithm waits a little bit before acknowledging a received packet.
                // This is usually a good idea, but not if your program sends small packets and cares about low latency.
                s.set_nodelay(true)?;
                let args = args.clone();
                let map = map.clone();
                let log = log.clone();

                // use the new --singlethread command line argument to set this
                if args.singlethread {
                    handle_client(args, s, map, log);
                } else {
                    std::thread::spawn(move || {
                        handle_client(args, s, map, log);
                    });
                }
            }
            Err(e) => eprintln!("Connection failed: {}", e),
        }
    }
    Ok(())
}
