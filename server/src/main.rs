use database::database::{insert_or_update_file, list_files};
use std::{
    fs,
    io::{self, Read, Seek, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc::{self, channel},
    thread,
    time::{Duration, Instant},
};

use crate::database::database::delete_file;

mod database;
// default msg = command <arg1> <arg2> <arg3> ...
const SERVER_ADDR: &str = "192.168.0.5:5000";
const FILES_DIR: &str = "./files";
const BUFFER_SIZE: usize = 16 * 1024;
const ACK: &[u8] = b"OK";
// server command map
const UPLOAD_CMD: u8 = 1;
const SEARCH_CMD: u8 = 2;
const DELETE_CMD: u8 = 3;
const LIST_CMD: u8 = 4;

fn handle_connection(mut stream: TcpStream) -> io::Result<()> {
    let mut buf = [0u8; 1]; // Command buffer
    if let Ok(_) = stream.read_exact(&mut buf) {
        match buf[0] {
            UPLOAD_CMD => upload_file(&mut stream),
            SEARCH_CMD => search_files(&mut stream),
            DELETE_CMD => delete_file_cmd(&mut stream),
            LIST_CMD => list_files_cmd(&mut stream),
            _ => send_message(&mut stream, "Invalid command"),
        }
    } else {
        send_message(&mut stream, "Error receiving command")
    }
}

fn upload_file(stream: &mut TcpStream) -> io::Result<()> {
    let name = recv_message(stream).unwrap_or_else(|e| {
        println!("Error receiving message: {}", e);
        close_connection(stream);
        return String::new();
    });
    send_ack(stream).unwrap_or_else(|e| {
        println!("Error sending ACK: {}", e);
        close_connection(stream);
    });

    recv_file(stream, &name).unwrap_or_else(|e| {
        println!("Error receiving file: {}", e);
        close_connection(stream);
        return 0;
    });

    insert_or_update_file(&name, &format!("{}/{}", FILES_DIR, name)).unwrap_or_else(|e| {
        println!("Error sending ACK: {}", e);
        close_connection(stream);
    });
    Ok(())
}

fn search_files(stream: &mut TcpStream) -> io::Result<()> {
    let search_term = recv_message(stream).unwrap_or_else(|e| {
        println!("Error receiving message: {}", e);
        close_connection(stream);
        return String::new();
    });
    send_ack(stream).unwrap_or_else(|e| {
        println!("Error sending ACK: {}", e);
        close_connection(stream);
    });

    // Iterate over every file in the directory
    let entries = fs::read_dir(FILES_DIR).unwrap();
    let start_time = Instant::now();

    // search each file
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_file() {
            println!("Searching in file: {}", path.display());
            search_in_file(
                stream,
                &path.display().to_string(),
                search_term
                    .to_lowercase()
                    .trim_matches(|c: char| !c.is_alphanumeric()),
            )
            .unwrap_or_else(|e| {
                println!("Error searching in file: {}", e);
                close_connection(stream);
                return 0;
            });
        }
    }

    let elapsed_time = start_time.elapsed();
    send_message(stream, &format!("done: {:?}", elapsed_time)).unwrap_or_else(|e| {
        println!("Error sending message: {}", e);
    });
    send_ack(stream).unwrap_or_else(|e| {
        println!("Error sending ACK: {}", e);
        close_connection(stream);
    });
    Ok(())
}
fn search_in_file(stream: &mut TcpStream, file_name: &str, search_term: &str) -> io::Result<u64> {
    // get file in files folder
    let mut file = std::fs::File::open(file_name)?;
    let file_size = file.metadata()?.len() as u64;
    let mut buffer = [0; 1024 * 1024];
    let overlap = search_term.len();

    let update_interval = Duration::from_millis(500);
    let mut last_update = Instant::now();
    let mut total_bytes_read = 0 as u64;
    send_message(
        stream,
        &format!(
            "searching: {}, {}", // Progress percentage
            file_name, file_size as u64
        ),
    )?;
    while let Ok(bytes_read) = file.read(&mut buffer) {
        total_bytes_read += bytes_read as u64;
        if (last_update.elapsed() > update_interval) {
            send_message(
                stream,
                &format!(
                    "update: {}, {}", // Progress percentage
                    file_name, total_bytes_read as u64
                ),
            )?;
            last_update = Instant::now();
        }

        let search_term = search_term.to_string();

        let content = String::from_utf8_lossy(&buffer[..bytes_read]);
        for (index, _) in content
            .to_lowercase()
            .match_indices(search_term.to_lowercase().as_str())
        {
            let start_index = index;
            let end_index = std::cmp::min(content.len(), start_index + overlap + 10);
            let snippet = content[start_index..end_index].to_string();
            send_message(
                stream,
                &format!(
                    "found: {}, {}, {}", // Progress percentage
                    file_name, index, snippet
                ),
            )?;
        }

        if bytes_read == overlap {
            break;
        }
        file.seek(std::io::SeekFrom::Current(-(overlap as i64)))?;
    }
    Ok(file_size)
}

fn delete_file_cmd(stream: &mut TcpStream) -> io::Result<()> {
    let name = recv_message(stream).unwrap_or_else(|e| {
        println!("Error receiving message: {}", e);
        close_connection(stream);
        return String::new();
    });
    send_ack(stream).unwrap_or_else(|e| {
        println!("Error sending ACK: {}", e);
        close_connection(stream);
    });

    delete_file(name.as_str()).unwrap_or_else(|e| {
        println!("Error deleting file from db: {}", e);
        close_connection(stream);
    });

    let file_path = format!("{}/{}", FILES_DIR, name);
    println!("Deleting file: {}", file_path);
    match fs::remove_file(&file_path) {
        Ok(_) => {
            println!("File deleted: {}", file_path);

            send_ack(stream).unwrap_or_else(|e| {
                println!("Error sending ACK: {}", e);
                close_connection(stream);
            });
        }
        Err(e) => {
            println!("Error deleting file: {}", e);
            send_message(stream, &format!("error: {}", e)).unwrap_or_else(|e| {
                println!("Error sending message: {}", e);
            });
            close_connection(stream);
        }
    }
    Ok(())
}

fn list_files_cmd(stream: &mut TcpStream) -> io::Result<()> {
    send_ack(stream).unwrap_or_else(|e| {
        println!("Error sending ACK: {}", e);
        close_connection(stream);
    });
    let db_files = list_files();
    match db_files {
        Ok(files) => {
            for (name, path) in files {
                println!("Listing files: {}", name);
                send_message(stream, &format!("file: {}", name).as_str()).unwrap_or_else(|e| {
                    println!("Error sending message: {}", e);
                });
            }
            send_message(stream, "done:").unwrap_or_else(|e| {
                println!("Error sending message: {}", e);
            });
            send_ack(stream).unwrap_or_else(|e| {
                println!("Error sending ACK: {}", e);
                close_connection(stream);
            });
        }
        Err(e) => {
            println!("Error listing files: {}", e);
            close_connection(stream);
        }
    }
    Ok(())
}

fn send_message(stream: &mut TcpStream, message: &str) -> io::Result<()> {
    let message_len = message.len();
    let message_len_bytes = message_len.to_be_bytes();
    stream.write_all(&message_len_bytes)?;
    stream.flush()?;
    stream.write_all(message.as_bytes())?;
    stream.flush()?;
    Ok(())
}

fn send_ack(stream: &mut TcpStream) -> io::Result<()> {
    stream.write_all(ACK)?;
    stream.flush()?;
    Ok(())
}

fn recv_message(stream: &mut TcpStream) -> io::Result<String> {
    let mut length_bytes = [0u8; 8];
    stream.read_exact(&mut length_bytes)?;
    let length = u64::from_be_bytes(length_bytes) as usize;
    let mut buffer = vec![0; length];
    stream.read_exact(&mut buffer)?;
    Ok(String::from_utf8(buffer).unwrap())
}

fn recv_file(stream: &mut TcpStream, name: &str) -> io::Result<u32> {
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
    Ok(received as u32)
}

fn close_connection(stream: &mut TcpStream) {
    stream
        .shutdown(std::net::Shutdown::Both)
        .unwrap_or_else(|e| {
            println!("Error shutting down connection: {}", e);
        });
}

fn main() {
    let listener = TcpListener::bind(SERVER_ADDR).unwrap_or_else(|e| {
        println!("Error binding to address: {}", e);
        panic!();
    });
    println!("Server listening on: {}", SERVER_ADDR);

    for stream in listener.incoming() {
        let stream = stream.unwrap();
        thread::spawn(|| {
            handle_connection(stream);
        });
    }
}
