use database::database::{get_file, list_files};
use tokio::fs::{self, File};
use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use crate::database::database::{delete_file, init, insert_or_update_file};
use std::cmp::max;
use std::io::Write;
use std::path::Path;
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

async fn handle_client(mut stream: TcpStream) -> io::Result<()> {
    let mut buf = [0u8; 1]; // Command buffer
    if let Ok(_) = stream.read_exact(&mut buf).await {
        match buf[0] {
            UPLOAD_CMD => upload_file(&mut stream).await,
            SEARCH_CMD => search_files(&mut stream).await,
            DELETE_CMD => delete_file_cmd(&mut stream).await,
            LIST_CMD => list_files_cmd(&mut stream).await,
            _ => send_message(&mut stream, "Invalid command").await,
        }
    } else {
        send_message(&mut stream, "Error receiving command").await
    }
}

async fn upload_file(mut stream: &mut TcpStream) -> io::Result<()> {
    let name = recv_message(&mut stream).await.unwrap_or_else(|e| {
        println!("Error receiving message: {}", e);
        close_connection(&mut stream);
        return String::new();
    });
    send_ack(&mut stream).await.unwrap_or_else(|e| {
        println!("Error sending ACK: {}", e);
        close_connection(&mut stream);
    });

    recv_file(&mut stream, &name).await.unwrap_or_else(|e| {
        println!("Error receiving file: {}", e);
        close_connection(&mut stream);
        return 0;
    });

    insert_or_update_file(&name, &format!("{}/{}", FILES_DIR, name)).unwrap_or_else(|e| {
        println!("Error sending ACK: {}", e);
        close_connection(&mut stream);
    });
    Ok(())
}

async fn search_files(mut stream: &mut TcpStream) -> io::Result<()> {
    let search_term = recv_message(&mut stream).await.unwrap_or_else(|e| {
        println!("Error receiving message: {}", e);
        close_connection(&mut stream);
        return String::new();
    });
    send_ack(&mut stream).await.unwrap_or_else(|e| {
        println!("Error sending ACK: {}", e);
        close_connection(&mut stream);
    });
    // Calculate total bytes in all files to handle progress updates accurately
    let total_bytes = calculate_total_bytes(FILES_DIR).await.unwrap_or_else(|e| {
        println!("Error calculating total bytes{}", e);
        close_connection(&mut stream);
        return 0;
    });

    // Iterate over every file in the directory
    let mut entries = fs::read_dir(FILES_DIR).await.unwrap();
    let mut processed_bytes = 0u64;
    let start_time = Instant::now();

    while let Some(entry) = entries.next_entry().await.unwrap() {
        let path = entry.path();
        if path.is_file() {
            println!("Searching in file: {}", path.display());
            processed_bytes += search_in_file(
                &mut stream,
                &path,
                search_term
                    .to_lowercase()
                    .trim_matches(|c: char| !c.is_alphanumeric()),
                total_bytes,
                processed_bytes,
            )
            .await
            .unwrap_or_else(|e| {
                println!("Error searching in file: {}", e);
                close_connection(&mut stream);
                return 0;
            });
        }
    }

    let elapsed_time = start_time.elapsed();
    send_message(&mut stream, &format!("done: {:?}", elapsed_time))
        .await
        .unwrap_or_else(|e| {
            println!("Error sending message: {}", e);
        });
    send_ack(&mut stream).await.unwrap_or_else(|e| {
        println!("Error sending ACK: {}", e);
        close_connection(&mut stream);
    });
    Ok(())
}

async fn delete_file_cmd(mut stream: &mut TcpStream) -> io::Result<()> {
    let name = recv_message(&mut stream).await.unwrap_or_else(|e| {
        println!("Error receiving message: {}", e);
        close_connection(&mut stream);
        return String::new();
    });
    send_ack(&mut stream).await.unwrap_or_else(|e| {
        println!("Error sending ACK: {}", e);
        close_connection(&mut stream);
    });

    delete_file(name.as_str()).unwrap_or_else(|e| {
        println!("Error deleting file from db: {}", e);
        close_connection(&mut stream);
    });

    let file_path = format!("{}/{}", FILES_DIR, name);
    println!("Deleting file: {}", file_path);
    match fs::remove_file(&file_path).await {
        Ok(_) => {
            println!("File deleted: {}", file_path);

            send_ack(&mut stream).await.unwrap_or_else(|e| {
                println!("Error sending ACK: {}", e);
                close_connection(&mut stream);
            });
        }
        Err(e) => {
            println!("Error deleting file: {}", e);
            send_message(&mut stream, &format!("error: {}", e))
                .await
                .unwrap_or_else(|e| {
                    println!("Error sending message: {}", e);
                });
            close_connection(&mut stream);
        }
    }
    Ok(())
}

async fn list_files_cmd(mut stream: &mut TcpStream) -> io::Result<()> {
    send_ack(&mut stream).await.unwrap_or_else(|e| {
        println!("Error sending ACK: {}", e);
        close_connection(&mut stream);
    });
    let db_files = list_files();
    match db_files {
        Ok(files) => {
            for (name, path) in files {
                println!("Listing files: {}", name);
                send_message(&mut stream, &format!("file: {}", name).as_str())
                    .await
                    .unwrap_or_else(|e| {
                        println!("Error sending message: {}", e);
                    });
            }
            send_message(&mut stream, "done:")
                .await
                .unwrap_or_else(|e| {
                    println!("Error sending message: {}", e);
                });
            send_ack(&mut stream).await.unwrap_or_else(|e| {
                println!("Error sending ACK: {}", e);
                close_connection(&mut stream);
            });
        }
        Err(e) => {
            println!("Error listing files: {}", e);
            close_connection(&mut stream);
        }
    }
    Ok(())
}

async fn calculate_total_bytes(dir: &str) -> io::Result<u64> {
    let mut entries = fs::read_dir(dir).await?;
    let mut total_size = 0u64;
    while let Some(entry) = entries.next_entry().await.unwrap() {
        let path = entry.path();
        if path.is_file() {
            total_size += fs::metadata(path).await?.len();
        }
    }
    Ok(total_size)
}

async fn search_in_file(
    stream: &mut TcpStream,
    file_path: &Path,
    search_term: &str,
    total_bytes: u64,
    already_processed_bytes: u64,
) -> io::Result<u64> {
    let mut file = File::open(file_path).await?;
    let file_size = file.metadata().await?.len() as u64;
    let mut reader = BufReader::new(file);

    let term_len = max(1024 * 512, search_term.len());
    let mut buffer = vec![0; term_len * 2];
    let mut global_position = 0u64; // Position within the current file
    let mut processed_bytes = 0u64; // Total processed bytes across all files
    let mut found_in_this_file = false;

    let mut bytes_read = reader.read(&mut buffer[term_len..]).await?;
    buffer.copy_within(term_len..term_len + bytes_read, 0);
    bytes_read += term_len;

    let update_interval = Duration::from_millis(200);
    let mut last_update = Instant::now();

    while bytes_read != term_len {
        if (last_update.elapsed() > update_interval) {
            send_message(
                stream,
                &format!(
                    "update: {}%", // Progress percentage
                    (already_processed_bytes + processed_bytes + global_position) as f64
                        / total_bytes as f64
                        * 100.0
                ),
            )
            .await?;
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
                )
                .await?;
                found_in_this_file = true;
            }
            // Get snippet of the line
            let start_index = index;
            let end_index = std::cmp::min(content.len(), start_index + search_term.len() + 10);
            let snippet = &content[start_index..end_index]
                .chars()
                .filter(|&c| c != '\n' && c != '\r')
                .collect::<String>();
            send_message(
                stream,
                &format!(
                    "found: {}, {}, {}", // file_path, global_position, snippet
                    file_path.to_string_lossy(),
                    global_position + index as u64,
                    snippet
                ),
            )
            .await?;
        }

        buffer.copy_within(term_len.., 0);

        global_position += term_len as u64;

        let new_bytes = reader.read(&mut buffer[term_len..]).await?;
        bytes_read = term_len + new_bytes;
    }

    processed_bytes += file_size; // Update total processed bytes after finishing the file

    Ok(processed_bytes)
}
async fn close_connection(stream: &mut TcpStream) {
    stream.shutdown().await.unwrap_or_else(|e| {
        println!("Error shutting down connection: {}", e);
        panic!();
    });
}

async fn send_message(stream: &mut TcpStream, message: &str) -> io::Result<()> {
    let message_len = message.len();
    let message_len_bytes = message_len.to_be_bytes();
    stream.write_all(&message_len_bytes).await?;
    stream.flush().await?;
    stream.write_all(message.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

async fn send_ack(stream: &mut TcpStream) -> io::Result<()> {
    stream.write_all(ACK).await?;
    stream.flush().await?;
    Ok(())
}

async fn recv_command(stream: &mut TcpStream) -> io::Result<u8> {
    let mut buffer = [0; 1];
    match stream.read_exact(&mut buffer).await {
        Ok(_) => Ok(buffer[0]),
        Err(e) => Err(e),
    }
}

async fn recv_message(stream: &mut TcpStream) -> io::Result<String> {
    let mut length_bytes = [0u8; 8];
    stream.read_exact(&mut length_bytes).await?;
    let length = u64::from_be_bytes(length_bytes) as usize;

    let mut buffer = vec![0; length];
    stream.read_exact(&mut buffer).await?;
    Ok(String::from_utf8(buffer).unwrap())
}

async fn recv_file(stream: &mut TcpStream, name: &str) -> io::Result<u64> {
    let mut length_bytes = [0u8; 8];
    stream.read_exact(&mut length_bytes).await?;
    let length = u64::from_be_bytes(length_bytes);

    let mut file = std::fs::File::create(format!("{}/{}", FILES_DIR, name))?;
    let mut received = 0u64;
    let mut buffer = [0; BUFFER_SIZE];
    while received < length {
        match stream.read(&mut buffer).await {
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
    send_ack(stream).await?;
    Ok(received)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 6)]
async fn main() -> io::Result<()> {
    init(); // Initialize any necessary components

    let listener = TcpListener::bind("192.168.0.5:5000").await?;
    println!("Server listening on port 5000");

    loop {
        let (stream, addr) = listener.accept().await?;
        println!("New connection: {}", addr);
        tokio::spawn(async move {
            handle_client(stream).await;
        });
    }
}
