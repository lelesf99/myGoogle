pub mod database {
    use sqlite::{Connection, State, Value};

    const DB_PATH: &str = "mygoogle.db";

    pub fn init() -> Result<Connection, sqlite::Error> {
        let conn = Connection::open(DB_PATH)?;
        conn.execute(
            "
            CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                path TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS words (
                id INTEGER PRIMARY KEY,
                word TEXT NOT NULL
            );
            
            CREATE TABLE IF NOT EXISTS file_words (
                id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL,
                word_id INTEGER NOT NULL,
                found_at UNSIGNED BIG INT NOT NULL,
                FOREIGN KEY (file_id) REFERENCES files (id),
                FOREIGN KEY (word_id) REFERENCES words (id)
            );
            ",
        )?;
        Ok(conn)
    }

    pub fn insert_file(name: &str, path: &str) -> Result<(), sqlite::Error> {
        let conn = Connection::open(DB_PATH)?;
        let query = "INSERT INTO files (name, path) VALUES (?, ?);";
        let mut statement = conn.prepare(query)?;
        statement.bind((1, name))?;
        statement.bind((2, path))?;
        statement.next()?;
        Ok(())
    }

    pub fn insert_or_update_file(name: &str, path: &str) -> Result<(), sqlite::Error> {
        let conn = Connection::open(DB_PATH)?;

        // Check if the record already exists
        let mut check_stmt = conn.prepare("SELECT COUNT(*) FROM files WHERE name = ? AND path = ?")?;
        check_stmt.bind((1, name))?;
        check_stmt.bind((2, path))?;

        // Execute the SELECT statement and get count
        let exists = match check_stmt.next()? {
            State::Row => check_stmt.read::<i64, usize>(0)? > 0,
            _ => false,
        };

        if exists {
            // Update the existing record
            let mut update_stmt = conn.prepare("UPDATE files SET path = ? WHERE name = ?")?;
            update_stmt.bind((1, path))?;
            update_stmt.bind((2, name))?;
            update_stmt.next()?;
        } else {
            // Insert a new record
            let mut insert_stmt = conn.prepare("INSERT INTO files (name, path) VALUES (?, ?)")?;
            insert_stmt.bind((1, name))?;
            insert_stmt.bind((2, path))?;
            insert_stmt.next()?;
        }

        Ok(())
    }

    pub fn get_file(name: &str) -> Result<(), sqlite::Error> {
        let conn = Connection::open(DB_PATH)?;
        let query = "SELECT name, path FROM files WHERE name = ?";
        let mut statement = conn.prepare(query)?;
        statement.bind((1, name))?;
        while let State::Row = statement.next()? {
            let name: String = statement.read(0)?;
            let path: String = statement.read(1)?;
            println!("name = {}, path = {}", name, path);
        }
        Ok(())
    }

    pub fn delete_file(name: &str) -> Result<(), sqlite::Error> {
        let conn = Connection::open(DB_PATH)?;
        let query = "DELETE FROM files WHERE name = ?";
        let mut statement = conn.prepare(query)?;
        statement.bind((1, name))?;
        statement.next()?;
        Ok(())
    }

    pub fn list_files() -> Result<Vec<(String, String)>, sqlite::Error> {
        let conn = Connection::open(DB_PATH)?;
        let query = "SELECT name, path FROM files";
        let mut statement = conn.prepare(query)?;
        let mut files = Vec::new();
        while let State::Row = statement.next()? {
            let name: String = statement.read(0)?;
            let path: String = statement.read(1)?;
            files.push((name, path));
        }
        Ok(files)
    }
}
