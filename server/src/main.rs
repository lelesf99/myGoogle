use database::database::{get_file, list_files};
use memmap::Mmap;
use threadpool::ThreadPool;

use crate::database::database::{delete_file, init, insert_or_update_file};
use std::cmp::max;
use std::fs::{self, File, ReadDir};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::windows::process;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

mod database;
// default msg = command <arg1> <arg2> <arg3> ...
const FILES_DIR: &str = "./files";
const BUFFER_SIZE: usize = 16 * 1024;
const DELIMITER: u8 = 0x0A;
const ACK: &[u8] = b"OK";
const NAK: &[u8] = b"NO";
// server command map
const UPLOAD_CMD: u8 = 1;
const SEARCH_CMD: u8 = 2;
const DELETE_CMD: u8 = 3;
const LIST_CMD: u8 = 4;

fn handle_client(mut stream: TcpStream) {
    let command = recv_command(&mut stream).unwrap_or_else(|e| {
        println!("Error receiving command: {}", e);
        return 0;
    });

    match command {
        UPLOAD_CMD => {
            let name = recv_message(&mut stream).unwrap_or_else(|e| {
                println!("Error receiving message: {}", e);
                close_connection(&mut stream);
                return String::new();
            });
            send_ack(&mut stream).unwrap_or_else(|e| {
                println!("Error sending ACK: {}", e);
                close_connection(&mut stream);
            });

            recv_file(&mut stream, &name).unwrap_or_else(|e| {
                println!("Error receiving file: {}", e);
                close_connection(&mut stream);
                return 0;
            });

            insert_or_update_file(&name, &format!("{}/{}", FILES_DIR, name)).unwrap_or_else(|e| {
                println!("Error sending ACK: {}", e);
                close_connection(&mut stream);
            });
        }
        SEARCH_CMD => {
            let search_term = recv_message(&mut stream).unwrap_or_else(|e| {
                println!("Error receiving message: {}", e);
                close_connection(&mut stream);
                return String::new();
            });
            send_ack(&mut stream).unwrap_or_else(|e| {
                println!("Error sending ACK: {}", e);
                close_connection(&mut stream);
            });
            // Calculate total bytes in all files to handle progress updates accurately
            let total_bytes = calculate_total_bytes(FILES_DIR).unwrap_or_else(|e| {
                println!("Error calculating total bytes{}", e);
                close_connection(&mut stream);
                return 0;
            });

            // Iterate over every file in the directory
            let entries = fs::read_dir(FILES_DIR).unwrap();
            let mut processed_bytes = 0u64;
            let start_time = Instant::now();

            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    println!("Searching in file: {}", path.display());
                    processed_bytes += search_in_file(
                        &mut stream,
                        &path,
                        &search_term,
                        total_bytes,
                        processed_bytes,
                    )
                    .unwrap_or_else(|e| {
                        println!("Error searching in file: {}", e);
                        close_connection(&mut stream);
                        return 0;
                    });
                }
            }

            let elapsed_time = start_time.elapsed();
            send_message(&mut stream, &format!("done: {:.2?}", elapsed_time)).unwrap_or_else(|e| {
                println!("Error sending message: {}", e);
            });
            send_ack(&mut stream).unwrap_or_else(|e| {
                println!("Error sending ACK: {}", e);
                close_connection(&mut stream);
            });
        }
        DELETE_CMD => {
            let name = recv_message(&mut stream).unwrap_or_else(|e| {
                println!("Error receiving message: {}", e);
                close_connection(&mut stream);
                return String::new();
            });
            send_ack(&mut stream).unwrap_or_else(|e| {
                println!("Error sending ACK: {}", e);
                close_connection(&mut stream);
            });

            delete_file(name.as_str()).unwrap_or_else(|e| {
                println!("Error deleting file from db: {}", e);
                close_connection(&mut stream);
            });

            let file_path = format!("{}/{}", FILES_DIR, name);
            println!("Deleting file: {}", file_path);
            match fs::remove_file(&file_path) {
                Ok(_) => {
                    println!("File deleted: {}", file_path);

                    send_ack(&mut stream).unwrap_or_else(|e| {
                        println!("Error sending ACK: {}", e);
                        close_connection(&mut stream);
                    });
                }
                Err(e) => {
                    println!("Error deleting file: {}", e);
                    send_message(&mut stream, &format!("error: {}", e)).unwrap_or_else(|e| {
                        println!("Error sending message: {}", e);
                    });
                    close_connection(&mut stream);
                }
            }
        }
        LIST_CMD => {
            send_ack(&mut stream).unwrap_or_else(|e| {
                println!("Error sending ACK: {}", e);
                close_connection(&mut stream);
            });
            let db_files = list_files();
            match db_files {
                Ok(files) => {
                    for (name, path) in files {
                        println!("Listing files: {}", name);
                        send_message(&mut stream, &format!("file: {}", name).as_str())
                            .unwrap_or_else(|e| {
                                println!("Error sending message: {}", e);
                            });
                    }
                    send_message(&mut stream, "done:").unwrap_or_else(|e| {
                        println!("Error sending message: {}", e);
                    });
                    send_ack(&mut stream).unwrap_or_else(|e| {
                        println!("Error sending ACK: {}", e);
                        close_connection(&mut stream);
                    });
                }
                Err(e) => {
                    println!("Error listing files: {}", e);
                    close_connection(&mut stream);
                }
            }
        }
        _ => {
            send_message(
                &mut stream,
                format!("Invalid command: {}", command).as_str(),
            )
            .unwrap_or_else(|e| {
                println!("Error sending message: {}", e);
            });
        }
    }
}

fn calculate_total_bytes(dir: &str) -> io::Result<u64> {
    let entries = fs::read_dir(dir)?;
    let mut total_size = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            total_size += fs::metadata(path)?.len();
        }
    }
    Ok(total_size)
}

fn search_in_file(
    stream: &mut TcpStream,
    file_path: &Path,
    search_term: &str,
    total_bytes: u64,
    already_processed_bytes: u64,
) -> io::Result<u64> {
    let mut file = File::open(file_path)?;
    let file_size = file.metadata()?.len() as u64;
    let mut reader = BufReader::new(file);

    let term_len = max(1024 * 512, search_term.len());
    let mut buffer = vec![0; term_len * 2];
    let mut global_position = 0u64; // Position within the current file
    let mut processed_bytes = 0u64; // Total processed bytes across all files
    let mut found_in_this_file = false;

    let mut bytes_read = reader.read(&mut buffer[term_len..])?;
    buffer.copy_within(term_len..term_len + bytes_read, 0);
    bytes_read += term_len;

    let update_interval = Duration::from_millis(200);
    let mut last_update = Instant::now();

    while bytes_read != term_len {
        
        if(last_update.elapsed() > update_interval) {
            send_message(
                stream,
                &format!(
                    "update: {}%", // Progress percentage
                    (already_processed_bytes + processed_bytes + global_position) as f64 / total_bytes as f64 * 100.0
                ),
            )?;
            last_update = Instant::now();
        }
        let content = String::from_utf8_lossy(&buffer[..bytes_read]);

        for (index, _) in content
            .to_lowercase()
            .match_indices(search_term.to_lowercase().as_str())
        {
            if !found_in_this_file {
                send_message(
                    stream,
                    &format!("found_in: {}", file_path.to_string_lossy()),
                )?;
                found_in_this_file = true;
            }
            // get snippet of the line
            let end_index = index + search_term.len() + 20;
            let snippet = &content[index..end_index];
            send_message(
                stream,
                &format!(
                    "found: {}, {}, {}", // file_path, global_position, snippet
                    file_path.to_string_lossy(),
                    global_position + index as u64,
                    snippet
                ),
            )?;
        }

        buffer.copy_within(term_len.., 0);

        global_position += term_len as u64;

        let new_bytes = reader.read(&mut buffer[term_len..])?;
        bytes_read = term_len + new_bytes;
    }

    processed_bytes += file_size; // Update total processed bytes after finishing the file

    Ok(processed_bytes)
}
fn close_connection(stream: &mut TcpStream) {
    stream
        .shutdown(std::net::Shutdown::Both)
        .unwrap_or_else(|e| {
            println!("Error shutting down connection: {}", e);
            panic!();
        });
}

fn send_message(stream: &mut TcpStream, message: &str) -> io::Result<()> {
    stream.write_all(message.as_bytes())?; // Write message
    stream.write_all(&[DELIMITER])?; // Write delimiter
    Ok(())
}

fn send_ack(stream: &mut TcpStream) -> io::Result<()> {
    stream.write_all(ACK)?;
    stream.flush()?;
    Ok(())
}

fn recv_command(stream: &mut TcpStream) -> io::Result<u8> {
    let mut buffer = [0; 1];
    match stream.read_exact(&mut buffer) {
        Ok(_) => Ok(buffer[0]),
        Err(e) => Err(e),
    }
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

fn recv_file(stream: &mut TcpStream, name: &str) -> io::Result<u64> {
    let mut length_bytes = [0u8; 8];
    stream.read_exact(&mut length_bytes)?;
    let length = u64::from_be_bytes(length_bytes);

    let mut file = std::fs::File::create(format!("{}/{}", FILES_DIR, name))?;
    let mut received = 0u64;
    let mut buffer = [0; BUFFER_SIZE];
    while received < length {
        match stream.read(&mut buffer) {
            Ok(0) => break, // End of file
            Ok(n) => {
                println!("Bytes read: {}", n);
                match file.write_all(&buffer[..n]) {
                    Ok(_) => (),
                    Err(e) => return Err(e),
                }
                received += n as u64;
            }
            Err(e) => {
                std::fs::remove_file(format!("{}/{}", FILES_DIR, name))?;
                return Err(e);
            }
        };
    }
    // Send an ACK back to the client
    send_ack(stream)?;
    Ok(received)
}

fn main() -> io::Result<()> {
    // Initialize any necessary components (if needed)
    init();

    // Create a TCP listener
    let listener = TcpListener::bind("127.0.0.1:5000")?;
    println!("Server listening on port 5000");

    // Create a thread pool with a fixed number of threads
    let pool = ThreadPool::new(4); // Number of threads can be adjusted

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("New connection: {}", stream.peer_addr().unwrap());
                // Use the pool to handle the connection
                pool.execute(move || {
                    handle_client(stream);
                });
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        }
    }

    // Gracefully shutdown the thread pool
    pool.join();
    Ok(())
}
