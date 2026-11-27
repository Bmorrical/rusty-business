//! The encrypted database that holds the boards on this computer.
//!
//! It is one SQLite file, `boards.db`, in the boards folder, opened by the
//! app itself with no server to run. SQLCipher encrypts the whole file, page
//! by page, so nothing in it reads as text without its key. On a Mac the
//! cipher is Apple's own, and on Windows and Linux it is OpenSSL's.
//!
//! The key is 32 random bytes, made the first time and kept in the operating
//! system's own key store: the Mac's Keychain, the Windows Credential
//! Manager, or the Secret Service on Linux. It is never written to a file,
//! a log, or the page.
//!
//! The database is the app's own copy of each board, not a board's backend:
//! a board still comes in as a file and goes out as one (`BOARD.md`).
//! Nothing here uses the network, and nothing here deletes a row.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use rusqlite::{Connection, OptionalExtension, params};

/// The database's file, in the boards folder.
pub const FILE: &str = "boards.db";
/// Where the key is kept in the system's key store: the app, then this.
const ACCOUNT: &str = "boards-key";
/// How long the key is, in bytes.
const KEY_BYTES: usize = 32;

/// The five kinds of thing the store keeps for a board, each by the board's
/// id: the snapshot, the person's drafts, the copy last synced, where the
/// board's file is, and what the runners wrote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Board,
    Drafts,
    Synced,
    Source,
    Work,
}

impl Kind {
    pub const ALL: [Kind; 5] = [
        Kind::Board,
        Kind::Drafts,
        Kind::Synced,
        Kind::Source,
        Kind::Work,
    ];

    /// The name the database knows the kind by.
    fn name(self) -> &'static str {
        match self {
            Kind::Board => "board",
            Kind::Drafts => "drafts",
            Kind::Synced => "synced",
            Kind::Source => "source",
            Kind::Work => "work",
        }
    }
}

/// Where the database's key comes from. The app uses the system's key store.
/// A test hands it a key of its own, so no test touches the person's.
pub trait Keys: Send + Sync {
    /// The key, or `None` when none has been made yet.
    fn get(&self) -> Result<Option<Vec<u8>>, String>;
    /// Keeps a new key.
    fn set(&self, key: &[u8]) -> Result<(), String>;
}

/// The operating system's own key store.
pub struct SystemKeys {
    service: String,
}

impl SystemKeys {
    pub fn new(service: &str) -> Self {
        SystemKeys {
            service: service.to_string(),
        }
    }

    fn entry(&self) -> Result<keyring::Entry, String> {
        keyring::Entry::new(&self.service, ACCOUNT)
            .map_err(|error| format!("The system's key store could not be opened: {error}."))
    }
}

impl Keys for SystemKeys {
    fn get(&self) -> Result<Option<Vec<u8>>, String> {
        match self.entry()?.get_secret() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(format!(
                "The key for the boards could not be read from the system's key store: {error}."
            )),
        }
    }

    fn set(&self, key: &[u8]) -> Result<(), String> {
        self.entry()?.set_secret(key).map_err(|error| {
            format!("The key for the boards could not be kept in the system's key store: {error}.")
        })
    }
}

/// The database, open.
pub struct Store {
    db: Mutex<Connection>,
}

/// A key as SQLCipher takes a raw one: `x'…'`, in hex.
fn raw(key: &[u8]) -> String {
    let mut text = String::with_capacity(key.len() * 2 + 3);
    text.push_str("x'");
    for byte in key {
        let _ = write!(text, "{byte:02x}");
    }
    text.push('\'');
    text
}

/// A new key, from the system's own source of random bytes.
fn new_key() -> Result<Vec<u8>, String> {
    let mut key = vec![0u8; KEY_BYTES];
    getrandom::fill(&mut key).map_err(|_| "Could not make a key for the boards.".to_string())?;
    Ok(key)
}

impl Store {
    /// Opens the database in `dir`, or makes it, with the key from `keys`.
    /// A key is made only when there is no database yet: a database with
    /// no key to open it is said, never written over.
    pub fn open(dir: &Path, keys: &dyn Keys) -> Result<Store, String> {
        let path = dir.join(FILE);
        let there = std::fs::symlink_metadata(&path).is_ok();
        let key = match keys.get()? {
            Some(key) if key.len() == KEY_BYTES => key,
            Some(_) => return Err("The key for the boards is not one this app made.".to_string()),
            None if there => {
                return Err(format!(
                    "{} is here, and its key is not in the system's key store. It was left as it is.",
                    path.display()
                ));
            }
            None => {
                let key = new_key()?;
                keys.set(&key)?;
                key
            }
        };
        Self::open_with(&path, &key)
    }

    fn open_with(path: &Path, key: &[u8]) -> Result<Store, String> {
        let failed =
            |error: rusqlite::Error| format!("Could not open the boards' database: {error}.");
        let db = Connection::open(path).map_err(failed)?;
        db.pragma_update(None, "key", raw(key)).map_err(failed)?;
        // A build without SQLCipher would take the key and ignore it. This
        // stops one from ever writing a board in the clear.
        let cipher: Option<String> = db
            .query_row("PRAGMA cipher_version", [], |row| row.get(0))
            .optional()
            .map_err(failed)?;
        if cipher.is_none_or(|version| version.is_empty()) {
            return Err("This build cannot encrypt the boards' database.".to_string());
        }
        // The first read is where a wrong key shows.
        db.query_row("SELECT count(*) FROM sqlite_master", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|_| "The boards' database does not open with its key.".to_string())?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS kept (
                kind TEXT NOT NULL,
                id TEXT NOT NULL,
                text TEXT NOT NULL,
                PRIMARY KEY (kind, id)
            )",
        )
        .map_err(failed)?;
        own(path);
        Ok(Store { db: Mutex::new(db) })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The text kept for a board, or `None` when there is none.
    pub fn get(&self, kind: Kind, id: &str) -> Result<Option<String>, String> {
        self.db()
            .query_row(
                "SELECT text FROM kept WHERE kind = ?1 AND id = ?2",
                params![kind.name(), id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("Could not read the boards' database: {error}."))
    }

    /// Keeps text for a board, in place of what was kept before.
    pub fn put(&self, kind: Kind, id: &str, text: &str) -> Result<(), String> {
        self.db()
            .execute(
                "INSERT INTO kept (kind, id, text) VALUES (?1, ?2, ?3)
                 ON CONFLICT (kind, id) DO UPDATE SET text = excluded.text",
                params![kind.name(), id, text],
            )
            .map(|_| ())
            .map_err(|error| format!("Could not write the boards' database: {error}."))
    }

    /// Keeps text for a board only when nothing is kept for it yet. Says
    /// whether it was kept.
    pub fn add(&self, kind: Kind, id: &str, text: &str) -> Result<bool, String> {
        self.db()
            .execute(
                "INSERT INTO kept (kind, id, text) VALUES (?1, ?2, ?3)
                 ON CONFLICT (kind, id) DO NOTHING",
                params![kind.name(), id, text],
            )
            .map(|rows| rows == 1)
            .map_err(|error| format!("Could not write the boards' database: {error}."))
    }

    /// The id of every board that has this kind of thing kept, in order.
    pub fn ids(&self, kind: Kind) -> Result<Vec<String>, String> {
        let db = self.db();
        let mut query = db
            .prepare("SELECT id FROM kept WHERE kind = ?1 ORDER BY id")
            .map_err(|error| format!("Could not read the boards' database: {error}."))?;
        query
            .query_map(params![kind.name()], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(|error| format!("Could not read the boards' database: {error}."))
    }
}

/// Only the person who owns the file reads it, and the files SQLite keeps
/// beside it while it writes.
fn own(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A key store of the test's own, so no test reads or writes the
    /// person's Keychain.
    #[derive(Default)]
    pub struct TestKeys(pub Mutex<Option<Vec<u8>>>);

    impl Keys for TestKeys {
        fn get(&self) -> Result<Option<Vec<u8>>, String> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn set(&self, key: &[u8]) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(key.to_vec());
            Ok(())
        }
    }

    fn scratch() -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rusty-business-store-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_key_is_made_once_and_kept_in_the_key_store() {
        let dir = scratch();
        let keys = TestKeys::default();
        Store::open(&dir, &keys).unwrap();
        let key = keys.get().unwrap().unwrap();
        assert_eq!(key.len(), KEY_BYTES);
        Store::open(&dir, &keys).unwrap();
        assert_eq!(
            keys.get().unwrap().unwrap(),
            key,
            "a second open made a new key"
        );
    }

    #[test]
    fn what_is_kept_comes_back_by_kind_and_id() {
        let dir = scratch();
        let store = Store::open(&dir, &TestKeys::default()).unwrap();
        store.put(Kind::Board, "errands", "one").unwrap();
        store.put(Kind::Drafts, "errands", "notes").unwrap();
        store.put(Kind::Board, "errands", "two").unwrap();
        assert_eq!(
            store.get(Kind::Board, "errands").unwrap().as_deref(),
            Some("two")
        );
        assert_eq!(
            store.get(Kind::Drafts, "errands").unwrap().as_deref(),
            Some("notes")
        );
        assert_eq!(store.get(Kind::Synced, "errands").unwrap(), None);
        assert!(!store.add(Kind::Board, "errands", "three").unwrap());
        assert_eq!(
            store.get(Kind::Board, "errands").unwrap().as_deref(),
            Some("two")
        );
        assert!(store.add(Kind::Board, "chores", "x").unwrap());
        assert_eq!(store.ids(Kind::Board).unwrap(), ["chores", "errands"]);
    }

    #[test]
    fn the_file_is_encrypted() {
        let dir = scratch();
        let store = Store::open(&dir, &TestKeys::default()).unwrap();
        store
            .put(
                Kind::Board,
                "errands",
                "A board named Groceries and Errands",
            )
            .unwrap();
        drop(store);
        let bytes = std::fs::read(dir.join(FILE)).unwrap();
        assert!(
            !bytes.starts_with(b"SQLite format 3\0"),
            "the file is a plain SQLite file"
        );
        let found = |word: &[u8]| bytes.windows(word.len()).any(|at| at == word);
        assert!(
            !found(b"Groceries"),
            "a board's words are in the file in the clear"
        );
        assert!(
            !found(b"errands"),
            "a board's id is in the file in the clear"
        );
        assert!(
            !found(b"CREATE TABLE"),
            "the schema is in the file in the clear"
        );
    }

    #[test]
    fn a_wrong_key_does_not_open_it() {
        let dir = scratch();
        let keys = TestKeys::default();
        Store::open(&dir, &keys)
            .unwrap()
            .put(Kind::Board, "a", "b")
            .unwrap();
        let other = TestKeys(Mutex::new(Some(vec![7u8; KEY_BYTES])));
        let problem = Store::open(&dir, &other).err().unwrap();
        assert_eq!(problem, "The boards' database does not open with its key.");
        // Still whole, under its own key.
        let store = Store::open(&dir, &keys).unwrap();
        assert_eq!(store.get(Kind::Board, "a").unwrap().as_deref(), Some("b"));
    }

    #[test]
    fn a_database_with_no_key_is_left_as_it_is() {
        let dir = scratch();
        Store::open(&dir, &TestKeys::default())
            .unwrap()
            .put(Kind::Board, "a", "b")
            .unwrap();
        let before = std::fs::read(dir.join(FILE)).unwrap();
        let keys = TestKeys::default();
        let problem = Store::open(&dir, &keys).err().unwrap();
        assert!(
            problem.contains("its key is not in the system's key store"),
            "{problem}"
        );
        assert_eq!(
            keys.get().unwrap(),
            None,
            "a new key was made for a database that had one"
        );
        assert_eq!(std::fs::read(dir.join(FILE)).unwrap(), before);
    }

    #[test]
    fn a_key_of_the_wrong_length_is_refused() {
        let dir = scratch();
        let keys = TestKeys(Mutex::new(Some(vec![1u8; 8])));
        let problem = Store::open(&dir, &keys).err().unwrap();
        assert_eq!(problem, "The key for the boards is not one this app made.");
        assert!(std::fs::symlink_metadata(dir.join(FILE)).is_err());
    }

    #[test]
    fn the_key_is_given_to_sqlcipher_as_raw_hex() {
        assert_eq!(raw(&[0, 15, 255]), "x'000fff'");
    }

    #[cfg(unix)]
    #[test]
    fn only_its_owner_reads_the_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch();
        Store::open(&dir, &TestKeys::default()).unwrap();
        let mode = std::fs::metadata(dir.join(FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
