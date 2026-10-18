use std::io::{self, BufRead, Read, Write};
use vision_code_wasm::{process_json, MAX_REQUEST_BYTES};

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("vision-code [--lines]\nRead JSON from stdin and write a JSON response. --lines reads one request per line.\nEach request is limited to 65536 bytes. All generation runs locally.");
        return Ok(());
    }
    if !args.is_empty() && args != ["--lines"] {
        eprintln!("Usage: vision-code [--lines]");
        std::process::exit(2);
    }
    let stdin = io::stdin();
    let mut input = io::BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut output = stdout.lock();
    if args.is_empty() {
        let mut bytes = Vec::new();
        input
            .take((MAX_REQUEST_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        writeln!(output, "{}", process_json(&bytes))?;
    } else {
        loop {
            let mut bytes = Vec::new();
            let read = input
                .by_ref()
                .take((MAX_REQUEST_BYTES + 2) as u64)
                .read_until(b'\n', &mut bytes)?;
            if read == 0 {
                break;
            }
            if bytes.last() == Some(&b'\n') {
                bytes.pop();
            }
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            writeln!(output, "{}", process_json(&bytes))?;
            output.flush()?;
            if bytes.len() > MAX_REQUEST_BYTES {
                break;
            }
        }
    }
    Ok(())
}
