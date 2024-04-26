use crossterm::execute;
use crossterm::terminal::{Clear, ClearType};
use regex::bytes;
use tokio::sync::Mutex;
use tokio::time::sleep;
use core::time;
use std::io::{self, stdout, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::ops::Add;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use std::{fs::File, string};


// constants

const SERVER_ADDR: &str = "192.168.0.5:5000";
const BUFFER_SIZE: usize = 16 * 1024;
const DELIMITER: u8 = 0x0A;
const ACK: &[u8] = b"OK";
const NAK: &[u8] = b"NO";
// server command map
const UPLOAD_CMD: u8 = 1;
const SEARCH_CMD: u8 = 2;
const DELETE_CMD: u8 = 3;
const LIST_CMD: u8 = 4;

#[derive(Debug)]
struct FileState {
    name: String,
    occurrences: Vec<String>,
}

struct SearchState {
    progress: String,
    files: Vec<FileState>,
    last_update_lines: u32,
}

impl SearchState {
    fn new() -> Self {
        Self {
            files: Vec::new(),
            progress: "0.0".to_string(),
            last_update_lines: 0,
        }
    }

    fn update_progress(&mut self, progress: String) {
        self.progress = progress;
    }

    fn add_occurrence(&mut self, file_name: &str, occurrence: &str, snippet: &str) {
        for file in &mut self.files {
            if file.name == file_name.to_string() {
                file.occurrences
                    .push(occurrence.to_string() + " - " + snippet);
                return;
            }
        }
    }
    fn sort_occ(&mut self) {
        self.files
            .sort_by(|a, b| b.occurrences.len().cmp(&a.occurrences.len()));
    }

    fn display_short(&mut self) {
        // clear previous lines
        for _ in 0..self.last_update_lines {
            execute!(stdout(), crossterm::cursor::MoveUp(1)).unwrap();
            execute!(stdout(), crossterm::cursor::MoveToColumn(0)).unwrap();
            execute!(stdout(), Clear(ClearType::CurrentLine)).unwrap();
        }
        let mut line_counter = 0;
        println!("Search progress: {:.5}%", self.progress);
        line_counter += 1;
        for file in &self.files {
            println!("At File: {}, {} times", file.name, file.occurrences.len());
            line_counter += 1;
        }
        self.last_update_lines = line_counter;
    }

    fn display(&mut self) {
        // clear previous lines
        for _ in 0..self.last_update_lines {
            execute!(stdout(), crossterm::cursor::MoveUp(1)).unwrap();
            execute!(stdout(), crossterm::cursor::MoveToColumn(0)).unwrap();
            execute!(stdout(), Clear(ClearType::CurrentLine)).unwrap();
        }
        let mut line_counter = 0;
        println!("Search progress: {:.5}%", self.progress);
        line_counter += 1;
        for file in &self.files {
            println!("At File: {}, {} times", file.name, file.occurrences.len());
            line_counter += 1;
            let mut occ_counter = 0;
            for occurrence in &file.occurrences {
                if occ_counter > 10 {
                    println!("  ...");
                    line_counter += 1;
                    break;
                };
                println!("  At byte: {}", occurrence);
                occ_counter += 1;
                line_counter += 1;
            }
        }
        self.last_update_lines = line_counter;
    }
}

async fn handle_command(args: Vec<String>) -> Result<(), String> {
    match args[0].as_str() {
        "help" => {
            println!("Available commands:");
            println!("pwd - print current directory");
            println!("cd <dir> - change directory");
            println!("ls - list files in current directory");
            println!("clear - clear screen");
            println!("quit - quit program");
            println!("ping - ping server");
            println!("send <file> - send file to server");
            println!("recv <file> - receive file from server");
            println!("delete <file> - delete file from server");
            println!("list - list files on server");
            println!("test <n_requests> <full_duration> <search_term> - test server with multiple requests");
            Ok(())
        }
        // navigation commands
        "pwd" => {
            println!(
                "Current directory: {}",
                std::env::current_dir().unwrap().display()
            );
            Ok(())
        }
        "cd" => {
            println!("Changing directory to: {}", args[1]);
            // check if directory exists
            let path = std::path::Path::new(&args[1]);
            if !path.exists() {
                return Err(format!("Directory does not exist: {}", args[1]));
            }
            std::env::set_current_dir(&args[1]);
            Ok(())
        }
        "ls" => {
            println!("Listing files in current directory");
            let paths = std::fs::read_dir(".").unwrap();
            for path in paths {
                println!("Name: {}", path.unwrap().path().display());
            }
            Ok(())
        }
        "clear" => {
            print!("\x1B[2J\x1B[1;1H");
            Ok(())
        }
        "quit" => {
            println!("Quitting program");
            std::process::exit(0);
        }

        // Server control commands
        "upload" => {
            if args.len() < 2 {
                return Err("No file specified".to_string());
            }
            if !std::path::Path::new(&args[1]).exists() {
                return Err(format!("File does not exist: {}", args[1]));
            }
            let mut stream = TcpStream::connect(SERVER_ADDR).unwrap();
            if !stream.peer_addr().is_ok() {
                return Err(format!("Error connecting to server: {}", SERVER_ADDR));
            }
            send_command(&mut stream, UPLOAD_CMD);
            send_message(&mut stream, args[1].clone().as_str());

            if let Err(e) = wait_for_ack(&mut stream) {
                return Err(format!("Failed to receive ACK1: {}", e));
            }

            send_file(&mut stream, args[1].clone());

            if let Err(e) = wait_for_ack(&mut stream) {
                return Err(format!("Failed to receive ACK2: {}", e));
            } else {
                Ok(())
            }
        }
        "search" => {
            let mut stream = TcpStream::connect(SERVER_ADDR).expect("Failed to connect");
            send_command(&mut stream, SEARCH_CMD);

            let search_string = args[1].clone();
            send_message(&mut stream, search_string.as_str());

            if let Err(e) = wait_for_ack(&mut stream) {
                return Err(format!("Failed to receive ACK1: {}", e));
            }
            // Enables raw mode to control the cursor better
            let mut stdout = stdout();
            let start_time = Instant::now();
            let mut search_state = SearchState::new();

            loop {
                match recv_message(&mut stream) {
                    Ok(message) => {
                        if message.starts_with("found_in:") {
                            let file_name = message.replace("found_in: ", ""); // Save as a new String
                            let mut file_state = FileState {
                                name: file_name,
                                occurrences: Vec::new(),
                            };
                            search_state.files.push(file_state);
                        } else if message.starts_with("found:") {
                            // "found: {}, {}" format
                            let mut params = message.replace("found: ", "");
                            let mut parts = params.split(", ");
                            let file_name = parts.next().unwrap();
                            let byte = parts.next().unwrap();
                            let snippet = parts.next().unwrap();
                            // // find file by name
                            search_state.add_occurrence(file_name, byte, snippet);
                            search_state.sort_occ();
                        } else if message.starts_with("update:") {
                            let update_message = message.replace("update: ", ""); // Save as a new String
                            let progress = update_message.trim();
                            search_state.update_progress(progress.to_string());
                            search_state.display_short();
                        } else if message.starts_with("done:") {
                            let elapsed_time = start_time.elapsed();
                            search_state.display();
                            println!("Search completed in {:.2?}.", elapsed_time);
                            break;
                        }
                    }
                    Err(e) => {
                        println!("Error: {}", e);
                        break;
                    }
                }
            }
            Ok(())
        }
        "delete" => {
            let mut stream = TcpStream::connect(SERVER_ADDR).unwrap();
            if !stream.peer_addr().is_ok() {
                return Err(format!("Error connecting to server: {}", SERVER_ADDR));
            }
            send_command(&mut stream, DELETE_CMD);
            send_message(&mut stream, args[1].clone().as_str());

            if let Err(e) = wait_for_ack(&mut stream) {
                return Err(format!("Failed to receive ACK1: {}", e));
            }

            if let Err(e) = wait_for_ack(&mut stream) {
                return Err(format!("Failed to receive ACK2: {}", e));
            } else {
                Ok(())
            }
        }
        "list" => {
            let mut stream = TcpStream::connect(SERVER_ADDR).unwrap();
            if !stream.peer_addr().is_ok() {
                return Err(format!("Error connecting to server: {}", SERVER_ADDR));
            }
            send_command(&mut stream, LIST_CMD);

            if let Err(e) = wait_for_ack(&mut stream) {
                return Err(format!("Failed to receive ACK1: {}", e));
            }

            loop {
                match recv_message(&mut stream) {
                    Ok(message) => {
                        if message.starts_with("done:") {
                            break;
                        } else {
                            println!("{}", message);
                        }
                    }
                    Err(e) => {
                        println!("Error: {}", e);
                        break;
                    }
                }
            }
            Ok(())
        }
        "test" => {
            if args.len() < 4 {
                return Err("Not enough arguments".to_string());
            }
            let n_requests = args[1].parse::<usize>().unwrap();
            let full_duration = args[2].parse::<u64>().unwrap();
            let search_term = args[3].clone();
            test(n_requests, full_duration, search_term).await;
            Ok(())
        }
        _ => {
            return Err(format!("Unknown command: {}", args[0]));
        }
    }
}

fn send_message(stream: &mut TcpStream, message: &str) -> io::Result<()> {
    stream.write_all(message.as_bytes())?; // Write message
    stream.write_all(&[DELIMITER])?; // Write delimiter
    Ok(())
}

fn send_command(stream: &mut TcpStream, command: u8) -> io::Result<()> {
    match stream.write(&[command]) {
        Ok(_) => (),
        Err(e) => return Err(e),
    }
    stream.flush()?;
    Ok(())
}

fn recv_message(stream: &mut TcpStream) -> io::Result<String> {
    let mut reader = BufReader::new(stream);
    let mut message = String::new();
    reader.read_line(&mut message)?;
    if let Some('\n') = message.chars().last() {
        message.pop(); // Remove newline character at the end
    }
    if let Some('\r') = message.chars().last() {
        message.pop(); // Remove carriage return if present (for Windows compatibility)
    }
    Ok(message)
}

fn send_file(stream: &mut TcpStream, file: String) -> io::Result<()> {
    let mut file = match File::open(file) {
        Ok(file) => file,
        Err(e) => return Err(e),
    };
    let file_size = file.metadata()?.len();
    stream.write_all(&file_size.to_be_bytes())?;

    let mut buffer = [0u8; BUFFER_SIZE];
    let file_size = file.metadata()?.len();
    let mut byte_count = 0;
    while let Ok(n) = file.read(&mut buffer) {
        if n == 0 {
            break; // End of file
        }
        send_chunk(stream, &buffer[..n])?;
        byte_count += n as u64;
        let progress = (byte_count as f64 / file_size as f64) * 100.0;
        println!("Uploading: {:.2}%", progress);
        execute!(stdout(), crossterm::cursor::MoveUp(1)).unwrap();
        execute!(stdout(), crossterm::cursor::MoveToColumn(0)).unwrap();
        execute!(stdout(), Clear(ClearType::CurrentLine)).unwrap();
    }
    wait_for_ack(stream)?;
    Ok(())
}

fn send_chunk(stream: &mut TcpStream, chunk: &[u8]) -> io::Result<()> {
    stream.write_all(&chunk)?;
    Ok(())
}

fn wait_for_ack(stream: &mut TcpStream) -> io::Result<()> {
    let mut ack = [0u8; 2];
    match stream.read_exact(&mut ack) {
        Ok(_) => {
            if ack == [79, 75] {
                return Ok(());
            } else {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid ack"));
            }
        }
        Err(e) => {
            return Err(e);
        }
    }
}


async fn test(n_requests: usize, full_duration: u64, search_term: String) {
    let server_addr = Arc::new(SERVER_ADDR.to_string());
    let search_term = Arc::new(search_term);
    let interval = Duration::from_secs(full_duration) / n_requests as u32;

    // Shared state across tasks for accumulating time
    let time_acc = Arc::new(Mutex::new(Duration::from_secs(0)));

    let mut handles = Vec::new();

    for index in 0..n_requests {
        let server_addr = server_addr.clone();
        let search_term = search_term.clone();
        let time_acc = time_acc.clone();

        // Sleep before spawning the request task.
        tokio::time::sleep(interval).await;
        println!("sent n{index} request");
        let handle = tokio::spawn(async move {
            match send_search_request(&server_addr, &search_term, index as u32).await {
                Ok(time) => {
                    println!("Request {} completed in {:.2?}", index, time);
                    let mut time_acc_lock = time_acc.lock().await;
                    *time_acc_lock = *time_acc_lock + time;
                },
                Err(e) => {
                    eprintln!("Failed to send request: {:?}", e);
                }
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        if let Err(e) = handle.await {
            eprintln!("Failed during request handling: {:?}", e);
        }
    }

    let final_time_acc = time_acc.lock().await;
    println!("Sent {} requests, {:.2?} seconds average per request.", n_requests, final_time_acc.as_secs_f64() / n_requests as f64);
}

async fn send_search_request(server_addr: &str, search_term: &str, n_request: u32) -> tokio::io::Result<Duration> {
    let mut stream = TcpStream::connect(server_addr)?;
    let mut time = Instant::now();
    send_command(&mut stream, SEARCH_CMD)?;
    send_message(&mut stream, search_term)?;
    wait_for_ack(&mut stream)?;

    loop {
        match recv_message(&mut stream) {
            Ok(message) => {
                if !message.starts_with("done:") && !message.starts_with("update:") && !message.starts_with("found:") && !message.starts_with("found_in:") && message != ""{
                    println!("{n_request} received strange message{}", message);
                } else if (message.starts_with("done:")) {
                    return Ok(time.elapsed());
                }
            }
            Err(e) => {
                return Err(tokio::io::Error::new(tokio::io::ErrorKind::Other, e));
            }
        }
    }
}
#[tokio::main(worker_threads = 24000)]
async fn main() {
    loop {
        print!("Enter command: ");
        io::stdout().flush().unwrap(); // Ensure prompt is displayed before input

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        // split quoted strings
        let mut args: Vec<String> = input
            .trim()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        if (args[0].as_str() == "search") {
            args[1] = args[1..].join(" ");
            // discard the rest
            args.truncate(2);
        }

        if args.is_empty() {
            continue;
        }

        match handle_command(args).await {
            Ok(_) => (),
            Err(e) => println!("Error: {}", e),
        }
    }
}
