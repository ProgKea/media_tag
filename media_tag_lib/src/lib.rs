use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf, StripPrefixError};

#[derive(Debug)]
pub enum Error {
    SqliteError(rusqlite::Error),
    TagAlreadyExists(String),
    TagDoesNotExist(String),
    FileDoesNotExist(String),
    CouldNotDetermineMediaTagPath,
    IoError(std::io::Error),
    StripPrefixError(StripPrefixError),
    InvalidPathEncoding(PathBuf),
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::SqliteError(e)
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}
impl From<StripPrefixError> for Error {
    fn from(e: StripPrefixError) -> Self {
        Self::StripPrefixError(e)
    }
}
impl std::error::Error for Error {}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SqliteError(e) => write!(f, "Database error: {e}"),
            Self::TagAlreadyExists(t) => write!(f, "Tag \"{t}\" already exists"),
            Self::TagDoesNotExist(t) => write!(f, "Tag \"{t}\" does not exist"),
            Self::FileDoesNotExist(p) => write!(f, "File not found in database: {p}"),
            Self::CouldNotDetermineMediaTagPath => {
                write!(f, "Failed to determine library root path")
            }
            Self::IoError(e) => write!(f, "IO operation failed: {e}"),
            Self::StripPrefixError(e) => write!(f, "Path is not inside library root: {e}"),
            Self::InvalidPathEncoding(p) => {
                write!(f, "Path contains invalid UTF-8 characters: {}", p.display())
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct MediaTag {
    connection: Connection,
    root: PathBuf,
}

pub struct Tag {
    pub id: i64,
    pub name: String,
}

#[derive(Clone)]
pub struct Medium {
    pub id: i64,
    pub path: PathBuf,
    pub tags: Vec<i64>,
}

pub struct MediaTags {
    pub tags: HashMap<i64, String>,
    pub media: Vec<Medium>,
}

static SQL_SCRIPT: &str = include_str!("./db.sqlite");

impl MediaTag {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        let parent = path.parent().ok_or(Error::CouldNotDetermineMediaTagPath)?;

        let root = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        }
        .canonicalize()?;

        let connection = Connection::open(path)?;
        connection.execute_batch(SQL_SCRIPT)?;

        connection.execute("PRAGMA foreign_keys = ON;", [])?;

        Ok(Self { connection, root })
    }

    fn resolve_path_to_db_string<P: AsRef<Path>>(&self, path: P) -> Result<String> {
        let path = path.as_ref();
        let abs_path = path.canonicalize()?;
        let rel_path = abs_path.strip_prefix(&self.root)?;

        rel_path
            .to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| Error::InvalidPathEncoding(rel_path.to_path_buf()))
    }

    pub fn create_tag(&self, name: &str) -> Result<()> {
        let affected = self
            .connection
            .execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", (name,))?;

        if affected == 0 {
            return Err(Error::TagAlreadyExists(name.to_string()));
        }
        Ok(())
    }

    pub fn get_tags(&self) -> Result<Vec<Tag>> {
        let mut stmt = self.connection.prepare("SELECT id, name FROM tags")?;
        let tags = stmt
            .query_map([], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<Tag>, _>>()?;

        Ok(tags)
    }

    pub fn get_tag_id_map(&self) -> Result<HashMap<i64, String>> {
        let tags = self.get_tags()?;
        Ok(tags.into_iter().map(|t| (t.id, t.name)).collect())
    }

    fn get_medium_id_or_insert(&self, path_str: &str) -> Result<i64> {
        let id: i64 = self.connection.query_row(
            "INSERT INTO media (path) VALUES (?1)
             ON CONFLICT(path) DO UPDATE SET path=excluded.path
             RETURNING id",
            (path_str,),
            |row| row.get(0),
        )?;
        Ok(id)
    }

    pub fn add_tag<P: AsRef<Path>>(&self, path: P, tag_name: &str) -> Result<()> {
        let path_str = self.resolve_path_to_db_string(path)?;

        let medium_id = self.get_medium_id_or_insert(&path_str)?;

        let tag_id: i64 = self
            .connection
            .query_row("SELECT id FROM tags WHERE name = ?1", (tag_name,), |row| {
                row.get(0)
            })
            .optional()?
            .ok_or_else(|| Error::TagDoesNotExist(tag_name.to_string()))?;

        self.connection.execute(
            "INSERT OR IGNORE INTO media_tags(media_id, tag_id) VALUES (?1, ?2)",
            (medium_id, tag_id),
        )?;

        Ok(())
    }

    pub fn remove_tag<P: AsRef<Path>>(&self, path: P, tag_name: &str) -> Result<()> {
        let path_str = self.resolve_path_to_db_string(path)?;

        let medium_id: i64 = self
            .connection
            .query_row(
                "SELECT id FROM media WHERE path = ?1",
                (&path_str,),
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::FileDoesNotExist(path_str))?;

        let tag_id: i64 = self
            .connection
            .query_row("SELECT id FROM tags WHERE name = ?1", (tag_name,), |row| {
                row.get(0)
            })
            .optional()?
            .ok_or_else(|| Error::TagDoesNotExist(tag_name.to_string()))?;

        self.connection.execute(
            "DELETE FROM media_tags WHERE media_id = ?1 AND tag_id = ?2",
            (medium_id, tag_id),
        )?;

        Ok(())
    }

    pub fn load_media_tag(&self) -> Result<MediaTags> {
        let tag_id_map = self.get_tag_id_map()?;

        let mut stmt = self.connection.prepare(
            "SELECT m.id, m.path, GROUP_CONCAT(t.id, ',')
             FROM media m
             LEFT JOIN media_tags mt ON m.id = mt.media_id
             LEFT JOIN tags t ON mt.tag_id = t.id
             GROUP BY m.id",
        )?;

        let media = stmt
            .query_map([], |row| {
                let path_string: String = row.get(1)?;
                let path = self.root.join(path_string);

                let tag_id_string: Option<String> = row.get(2)?;
                let tags = match tag_id_string {
                    Some(s) => s.split(',').filter_map(|x| x.parse::<i64>().ok()).collect(),
                    None => Vec::new(),
                };

                Ok(Medium {
                    id: row.get(0)?,
                    path,
                    tags,
                })
            })?
            .collect::<std::result::Result<Vec<Medium>, _>>()?;

        Ok(MediaTags {
            media,
            tags: tag_id_map,
        })
    }
}
