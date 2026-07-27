//! SQLite wrapper for TheGamesDB metadata database.

use std::path::Path;

use rusqlite::{Connection, params};

/// Metadata for a single game from TheGamesDB.
#[derive(Debug, Clone)]
pub struct GameMetadata {
    pub game_id: i64,
    pub title: String,
    pub release_date: Option<String>,
    pub players: Option<u32>,
    pub overview: Option<String>,
    pub rating: Option<String>,
    pub genres: Vec<String>,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub images: Vec<ImageInfo>,
}

/// Image information from TheGamesDB.
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub image_type: String,
    pub side: Option<String>,
    pub filename: String,
}

/// Read-only connection to the TheGamesDB metadata SQLite database.
pub struct MetadataDb {
    conn: Connection,
    medium_base_url: String,
}

impl MetadataDb {
    /// Open the metadata database at the given path.
    ///
    /// Returns `None` if the file doesn't exist or can't be opened.
    pub fn open(path: &Path) -> Option<Self> {
        if !path.exists() {
            return None;
        }
        let conn =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;

        let medium_base_url = conn
            .query_row(
                "SELECT url FROM image_base_urls WHERE size = 'medium'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "https://cdn.thegamesdb.net/images/medium/".to_string());

        Some(Self {
            conn,
            medium_base_url,
        })
    }

    /// Return the CDN base URL for medium-sized images.
    pub fn medium_base_url(&self) -> &str {
        &self.medium_base_url
    }

    /// Load all game titles and IDs for fuzzy matching.
    ///
    /// Returns a vec of (game_id, title) pairs.
    pub fn all_titles(&self) -> Vec<(i64, String)> {
        let mut stmt = match self.conn.prepare("SELECT id, game_title FROM games") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    /// Load game titles and IDs for a specific platform.
    ///
    /// `platform_id` corresponds to TheGamesDB platform IDs (e.g. 7=NES, 4=GB).
    /// Returns a vec of (game_id, title) pairs. Alternate titles (stored as a
    /// JSON array in the `alternates` column) are included as additional
    /// candidates for the same game id, so ROMs named after an alternate
    /// title (e.g. "AD&D: Eye of the Beholder") still match.
    pub fn titles_for_platform(&self, platform_id: i64) -> Vec<(i64, String)> {
        let mut stmt = match self
            .conn
            .prepare("SELECT id, game_title, alternates FROM games WHERE platform_id = ?1")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let rows = stmt.query_map(params![platform_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        });

        let mut titles = Vec::new();
        if let Ok(rows) = rows {
            for (id, title, alternates) in rows.filter_map(Result::ok) {
                titles.push((id, title));
                if let Some(alts) = alternates
                    && let Ok(alts) = serde_json::from_str::<Vec<String>>(&alts)
                {
                    titles.extend(alts.into_iter().map(|alt| (id, alt)));
                }
            }
        }
        titles
    }

    /// Look up full metadata for a game by its ID.
    pub fn get_game(&self, game_id: i64) -> Option<GameMetadata> {
        let (title, release_date, players, overview, rating) = self
            .conn
            .query_row(
                "SELECT game_title, release_date, players, overview, rating \
                 FROM games WHERE id = ?1",
                params![game_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<u32>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .ok()?;

        let genres = self.query_genres(game_id);
        let developers = self.query_developers(game_id);
        let publishers = self.query_publishers(game_id);
        let images = self.query_images(game_id);

        Some(GameMetadata {
            game_id,
            title,
            release_date,
            players,
            overview,
            rating,
            genres,
            developers,
            publishers,
            images,
        })
    }

    fn query_genres(&self, game_id: i64) -> Vec<String> {
        let mut stmt = match self.conn.prepare(
            "SELECT g.name FROM genres g \
             JOIN game_genres gg ON g.id = gg.genre_id \
             WHERE gg.game_id = ?1 ORDER BY g.name",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        stmt.query_map(params![game_id], |row| row.get(0))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    fn query_developers(&self, game_id: i64) -> Vec<String> {
        let mut stmt = match self.conn.prepare(
            "SELECT d.name FROM developers d \
             JOIN game_developers gd ON d.id = gd.developer_id \
             WHERE gd.game_id = ?1 ORDER BY d.name",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        stmt.query_map(params![game_id], |row| row.get(0))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    fn query_publishers(&self, game_id: i64) -> Vec<String> {
        let mut stmt = match self.conn.prepare(
            "SELECT p.name FROM publishers p \
             JOIN game_publishers gp ON p.id = gp.publisher_id \
             WHERE gp.game_id = ?1 ORDER BY p.name",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        stmt.query_map(params![game_id], |row| row.get(0))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    fn query_images(&self, game_id: i64) -> Vec<ImageInfo> {
        let mut stmt = match self
            .conn
            .prepare("SELECT type, side, filename FROM images WHERE game_id = ?1")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        stmt.query_map(params![game_id], |row| {
            Ok(ImageInfo {
                image_type: row.get(0)?,
                side: row.get(1)?,
                filename: row.get(2)?,
            })
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
    }
}

impl GameMetadata {
    /// Get the front boxart filename, if any.
    pub fn front_boxart(&self) -> Option<&str> {
        self.images
            .iter()
            .find(|img| img.image_type == "boxart" && img.side.as_deref() == Some("front"))
            .map(|img| img.filename.as_str())
    }

    /// Get screenshot filenames.
    pub fn screenshots(&self) -> Vec<&str> {
        self.images
            .iter()
            .filter(|img| img.image_type == "screenshot")
            .map(|img| img.filename.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_db_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scripts")
            .join("metadata-scraper")
            .join("metadata.db")
    }

    #[test]
    fn open_returns_none_for_missing_file() {
        assert!(MetadataDb::open(Path::new("/nonexistent/metadata.db")).is_none());
    }

    #[test]
    fn open_succeeds_with_real_db() {
        let path = test_db_path();
        if !path.exists() {
            eprintln!("Skipping test: metadata.db not found at {path:?}");
            return;
        }
        assert!(MetadataDb::open(&path).is_some());
    }

    #[test]
    fn all_titles_returns_entries() {
        let path = test_db_path();
        if !path.exists() {
            return;
        }
        let db = MetadataDb::open(&path).unwrap();
        let titles = db.all_titles();
        assert!(!titles.is_empty(), "expected at least one title");
        assert!(
            titles.len() > 100,
            "expected many titles, got {}",
            titles.len()
        );
    }

    #[test]
    fn titles_for_platform_includes_alternate_titles() {
        let path = test_db_path();
        if !path.exists() {
            return;
        }
        let db = MetadataDb::open(&path).unwrap();
        // SNES game 284 "Advanced Dungeons & Dragons: Eye of the Beholder"
        // has the alternate title "AD&D: Eye of the Beholder"; both must be
        // offered as fuzzy-match candidates for the same game id.
        let titles = db.titles_for_platform(6);
        assert!(
            titles.iter().any(
                |(id, t)| *id == 284 && t == "Advanced Dungeons & Dragons: Eye of the Beholder"
            )
        );
        assert!(
            titles
                .iter()
                .any(|(id, t)| *id == 284 && t == "AD&D: Eye of the Beholder")
        );
    }

    #[test]
    fn medium_base_url_is_populated() {
        let path = test_db_path();
        if !path.exists() {
            return;
        }
        let db = MetadataDb::open(&path).unwrap();
        assert!(db.medium_base_url().starts_with("https://"));
        assert!(db.medium_base_url().contains("medium"));
    }

    #[test]
    fn get_game_returns_metadata() {
        let path = test_db_path();
        if !path.exists() {
            return;
        }
        let db = MetadataDb::open(&path).unwrap();
        // Game ID 5 is "Donkey Kong"
        let meta = db.get_game(5).expect("expected Donkey Kong to exist");
        assert_eq!(meta.title, "Donkey Kong");
        assert_eq!(meta.game_id, 5);
    }

    #[test]
    fn get_game_returns_none_for_unknown_id() {
        let path = test_db_path();
        if !path.exists() {
            return;
        }
        let db = MetadataDb::open(&path).unwrap();
        assert!(db.get_game(999_999_999).is_none());
    }

    #[test]
    fn get_game_includes_genres() {
        let path = test_db_path();
        if !path.exists() {
            return;
        }
        let db = MetadataDb::open(&path).unwrap();
        // Super Mario Bros. 3 (ID 112) should have genres
        let meta = db.get_game(112).expect("expected SMB3 to exist");
        assert!(
            !meta.genres.is_empty(),
            "expected genres for Super Mario Bros. 3"
        );
    }

    #[test]
    fn get_game_includes_images() {
        let path = test_db_path();
        if !path.exists() {
            return;
        }
        let db = MetadataDb::open(&path).unwrap();
        let meta = db.get_game(112).expect("expected SMB3 to exist");
        assert!(
            !meta.images.is_empty(),
            "expected images for Super Mario Bros. 3"
        );
    }

    #[test]
    fn front_boxart_extracts_correct_image() {
        let meta = GameMetadata {
            game_id: 1,
            title: "Test".to_string(),
            release_date: None,
            players: None,
            overview: None,
            rating: None,
            genres: vec![],
            developers: vec![],
            publishers: vec![],
            images: vec![
                ImageInfo {
                    image_type: "boxart".to_string(),
                    side: Some("back".to_string()),
                    filename: "back.jpg".to_string(),
                },
                ImageInfo {
                    image_type: "boxart".to_string(),
                    side: Some("front".to_string()),
                    filename: "front.jpg".to_string(),
                },
                ImageInfo {
                    image_type: "screenshot".to_string(),
                    side: None,
                    filename: "screen.jpg".to_string(),
                },
            ],
        };
        assert_eq!(meta.front_boxart(), Some("front.jpg"));
        assert_eq!(meta.screenshots(), vec!["screen.jpg"]);
    }
}
