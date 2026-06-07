use std::{
    env,
    io::{self, BufRead, BufReader, Write},
    net::TcpStream,
    time::{Duration, Instant},
};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const WHITE: &str = "\x1b[97m";

macro_rules! ok   { ($($t:tt)*) => { print!("{GREEN}[+]{RESET} "); println!($($t)*) } }
macro_rules! fail { ($($t:tt)*) => { print!("{RED}[-]{RESET} ");   println!($($t)*) } }
macro_rules! info { ($($t:tt)*) => { print!("{CYAN}[*]{RESET} ");   println!($($t)*) } }

struct Proxy {
    username: String,
    password: String,
    ip: String,
    port: u16,
}

impl Proxy {
    fn parse(input: &str) -> Result<Self, String> {
        let (creds, host) = input
            .split_once('@')
            .ok_or("missing '@' — expected username:password@ip:port")?;

        let (username, password) = creds.split_once(':').ok_or("missing ':' in credentials")?;

        let (ip, port_str) = host.rsplit_once(':').ok_or("missing ':' in host")?;

        let port = port_str
            .parse::<u16>()
            .map_err(|_| format!("invalid port '{port_str}'"))?;

        Ok(Self {
            username: username.to_string(),
            password: password.to_string(),
            ip: ip.to_string(),
            port,
        })
    }

    fn addr(&self) -> String {
        format!("{}:{}", self.ip, self.port)
    }
}

const PING_COUNT: u32 = 5;
const TIMEOUT: Duration = Duration::from_secs(5);

const PING_TARGETS: &[&str] = &[
    "google.com:80",
    "cloudflare.com:80",
    "amazon.com:80",
    "microsoft.com:80",
    "github.com:80",
    "facebook.com:80",
    "x.com:80",
    "reddit.com:80",
    "youtube.com:80",
    "wikipedia.org:80",
    "apple.com:80",
    "akamai.com:80",
];

fn tcp_ping(proxy: &Proxy, target: &str) -> Option<Duration> {
    let addr = proxy.addr().parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT).ok()?;
    stream.set_read_timeout(Some(TIMEOUT)).ok();

    let creds_b64 = base64_encode(format!("{}:{}", proxy.username, proxy.password).as_bytes());
    let request = format!(
        "CONNECT {target} HTTP/1.1\r\n\
         Host: {target}\r\n\
         Proxy-Authorization: Basic {creds_b64}\r\n\
         \r\n"
    );
    let start = Instant::now();
    stream.write_all(request.as_bytes()).ok()?;

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).ok()?;
    if !status_line.contains("200") {
        return None;
    }

    Some(start.elapsed())
}

fn direct_ping(target: &str) -> Option<Duration> {
    use std::net::ToSocketAddrs;
    let addr = target.to_socket_addrs().ok()?.next()?;
    let start = Instant::now();
    TcpStream::connect_timeout(&addr, TIMEOUT).ok()?;
    Some(start.elapsed())
}

fn http_connect_check(proxy: &Proxy) -> bool {
    let addr = match proxy.addr().parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let mut stream = match TcpStream::connect_timeout(&addr, TIMEOUT) {
        Ok(s) => s,
        Err(_) => return false,
    };
    stream.set_read_timeout(Some(TIMEOUT)).ok();

    let creds_b64 = base64_encode(format!("{}:{}", proxy.username, proxy.password).as_bytes());

    let target = PING_TARGETS[0];
    let request = format!(
        "CONNECT {target} HTTP/1.1\r\n\
         Host: {target}\r\n\
         Proxy-Authorization: Basic {creds_b64}\r\n\
         \r\n"
    );

    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    if reader.read_line(&mut status_line).is_err() {
        return false;
    }

    status_line.contains("200")
}

fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn print_help() {
    println!();
    println!(
        "  {BOLD}{WHITE}proxy-checker{RESET}  — check HTTP CONNECT proxies and measure latency"
    );
    println!();
    println!("  {BOLD}USAGE{RESET}");
    println!("    proxy-checker {CYAN}<proxy>{RESET}");
    println!("    proxy-checker {CYAN}<proxy>{RESET} {CYAN}<targets>{RESET}");
    println!("    proxy-checker --help");
    println!();
    println!("  {BOLD}ARGUMENTS{RESET}");
    println!("    {CYAN}<proxy>{RESET}");
    println!("      Proxy address in the form {BOLD}user:pass@ip:port{RESET}.");
    println!("      If omitted, the program prompts for it interactively.");
    println!();
    println!("    {CYAN}<targets>{RESET}");
    println!("      Comma-separated list of hosts to ping through the proxy.");
    println!(
        "      Each entry can be {BOLD}host{RESET} or {BOLD}host:port{RESET} (default port: 80)."
    );
    println!("      If omitted, the built-in list of {DEFAULT_N} targets is used.");
    println!();
    println!("  {BOLD}EXAMPLES{RESET}");
    println!("    proxy-checker user:pass@1.2.3.4:8080");
    println!("    proxy-checker user:pass@1.2.3.4:8080 example.com,httpbin.org:443");
    println!();
}

const DEFAULT_N: usize = PING_TARGETS.len();

fn normalize_target(s: &str) -> String {
    if s.contains(':') {
        s.to_string()
    } else {
        format!("{s}:80")
    }
}

fn main() {
    let mut args = env::args().skip(1);
    let first = args.next();

    if first.as_deref() == Some("--help") || first.as_deref() == Some("-h") {
        print_help();
        return;
    }

    let proxy_str = first.unwrap_or_else(|| {
        eprint!("{CYAN}proxy (user:pass@ip:port):{RESET} ");
        io::stdout().flush().ok();
        let mut buf = String::new();
        io::stdin().read_line(&mut buf).expect("stdin read failed");
        buf.trim().to_string()
    });

    let proxy = match Proxy::parse(proxy_str.trim()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{RED}error:{RESET} {e}");
            std::process::exit(1);
        }
    };

    let custom_targets: Vec<String> = args
        .next()
        .map(|s| s.split(',').map(|t| normalize_target(t.trim())).collect())
        .unwrap_or_default();

    let targets: Vec<&str> = if custom_targets.is_empty() {
        PING_TARGETS.to_vec()
    } else {
        custom_targets.iter().map(String::as_str).collect()
    };

    println!();
    println!("  {DIM}host{RESET}  {BOLD}{WHITE}{}{RESET}", proxy.addr());
    println!("  {DIM}user{RESET}  {YELLOW}{}{RESET}", proxy.username);
    println!();

    info!("checking HTTP CONNECT...");
    if !http_connect_check(&proxy) {
        fail!("HTTP CONNECT FAILED — wrong credentials or non-HTTP proxy");
        println!();
        return;
    }
    ok!("HTTP CONNECT OK");

    let n = targets.len();
    println!();
    info!("pinging {n} targets through proxy ({PING_COUNT}x each)...");
    println!();
    println!(
        "  {DIM}{:<20}  {:>9}  {:>9}  {:>10}{RESET}",
        "target", "proxy avg", "direct avg", "overhead"
    );

    for &target in &targets {
        let host = target.split(':').next().unwrap_or(target);
        let pings: Vec<Duration> = (0..PING_COUNT)
            .filter_map(|_| tcp_ping(&proxy, target))
            .collect();
        let directs: Vec<Duration> = (0..PING_COUNT)
            .filter_map(|_| direct_ping(target))
            .collect();

        if pings.is_empty() {
            println!("  {:<20}  {RED}timeout{RESET}", host);
            continue;
        }

        let avg_ms =
            pings.iter().map(|d| d.as_secs_f64() * 1000.0).sum::<f64>() / pings.len() as f64;
        let color = match avg_ms as u64 {
            0..=100 => GREEN,
            101..=300 => YELLOW,
            _ => RED,
        };
        let proxy_str = format!("{:.1} ms", avg_ms);

        if directs.is_empty() {
            println!(
                "  {:<20}  {color}{BOLD}{:>9}{RESET}  {:>9}  {:>10}",
                host, proxy_str, "n/a", "n/a"
            );
        } else {
            let direct_avg = directs
                .iter()
                .map(|d| d.as_secs_f64() * 1000.0)
                .sum::<f64>()
                / directs.len() as f64;
            let overhead = avg_ms - direct_avg;
            let sign = if overhead >= 0.0 { "+" } else { "" };
            let direct_str = format!("{:.1} ms", direct_avg);
            let overhead_str = format!("{sign}{:.1} ms", overhead);
            println!(
                "  {:<20}  {color}{BOLD}{:>9}{RESET}  {:>9}  {:>10}",
                host, proxy_str, direct_str, overhead_str
            );
        }
    }

    println!();
}
