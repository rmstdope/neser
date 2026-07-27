//! Fuzzy title matching between ROM names and TheGamesDB titles.

use strsim::jaro_winkler;

/// Minimum similarity score to consider a title match valid.
const SIMILARITY_THRESHOLD: f64 = 0.85;

/// Result of a fuzzy title match.
#[derive(Debug, Clone)]
pub struct TitleMatch {
    pub game_id: i64,
    pub db_title: String,
    pub score: f64,
}

/// Find the best matching game from the candidates for the given ROM title.
///
/// Returns `None` if no candidate exceeds the similarity threshold.
pub fn match_title(rom_title: &str, candidates: &[(i64, String)]) -> Option<TitleMatch> {
    let normalized_rom = normalize(rom_title);

    candidates
        .iter()
        .map(|(id, title)| {
            let normalized_db = normalize(title);
            let score = jaro_winkler(&normalized_rom, &normalized_db);
            (*id, title.as_str(), score)
        })
        .filter(|(_, _, score)| *score >= SIMILARITY_THRESHOLD)
        .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(game_id, db_title, score)| TitleMatch {
            game_id,
            db_title: db_title.to_string(),
            score,
        })
}

/// Normalize a title for comparison: lowercase, drop apostrophes, treat all
/// other punctuation as word separators (so slugified filenames like
/// "warios-woods" align with "Wario's Woods"), strip leading "the", and
/// collapse extra spaces.
fn normalize(title: &str) -> String {
    let lowered: String = title
        .to_lowercase()
        .chars()
        .filter(|c| !matches!(c, '\'' | '\u{2019}'))
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();

    let trimmed = lowered.split_whitespace().collect::<Vec<_>>().join(" ");

    trimmed.strip_prefix("the ").unwrap_or(&trimmed).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates() -> Vec<(i64, String)> {
        vec![
            (5, "Donkey Kong".to_string()),
            (112, "Super Mario Bros. 3".to_string()),
            (113, "The Legend of Zelda".to_string()),
            (121, "Kirby's Adventure".to_string()),
            (123, "Metroid".to_string()),
            (125, "Mega Man 5".to_string()),
            (135, "Castlevania".to_string()),
            (200, "Contra".to_string()),
            (300, "Tetris".to_string()),
        ]
    }

    #[test]
    fn exact_match() {
        let result = match_title("Super Mario Bros. 3", &candidates());
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.game_id, 112);
        assert!(m.score > 0.99);
    }

    #[test]
    fn case_insensitive_match() {
        let result = match_title("super mario bros. 3", &candidates());
        assert!(result.is_some());
        assert_eq!(result.unwrap().game_id, 112);
    }

    #[test]
    fn match_without_punctuation() {
        let result = match_title("Kirbys Adventure", &candidates());
        assert!(result.is_some());
        assert_eq!(result.unwrap().game_id, 121);
    }

    #[test]
    fn simple_exact_match() {
        let result = match_title("Metroid", &candidates());
        assert!(result.is_some());
        assert_eq!(result.unwrap().game_id, 123);
    }

    #[test]
    fn no_match_for_gibberish() {
        let result = match_title("XYZ Unknown Game 999", &candidates());
        assert!(result.is_none());
    }

    #[test]
    fn normalize_strips_punctuation_extra_spaces_and_the_prefix() {
        assert_eq!(normalize("Super Mario Bros. 3"), "super mario bros 3");
        assert_eq!(normalize("Kirby's  Adventure"), "kirbys adventure");
        assert_eq!(normalize("  Mega  Man  5  "), "mega man 5");
        assert_eq!(normalize("The Legend of Zelda"), "legend of zelda");
    }

    #[test]
    fn normalize_treats_separators_as_spaces() {
        // Slugified filenames use dashes as word separators; they must
        // normalize to the same string as the spaced title.
        assert_eq!(normalize("warios-woods"), "warios woods");
        assert_eq!(normalize("kirbys-dream-land-3"), "kirbys dream land 3");
        // '&' and ':' separate words too, and runs of separators collapse.
        assert_eq!(
            normalize("AD&D: Eye of the Beholder"),
            "ad d eye of the beholder"
        );
        assert_eq!(
            normalize("ad-d---eye-of-the-beholder"),
            "ad d eye of the beholder"
        );
    }

    #[test]
    fn slugified_filename_matches_spaced_title() {
        let cands = vec![
            (5, "Donkey Kong".to_string()),
            (76, "Wario's Woods".to_string()),
        ];
        let result = match_title("warios-woods", &cands);
        assert!(result.is_some());
        assert_eq!(result.unwrap().game_id, 76);
    }

    #[test]
    fn slugified_filename_matches_alternate_title_candidate() {
        // "ad-d---eye-of-the-beholder.sfc" must match via the alternate
        // title "AD&D: Eye of the Beholder" of SNES game 284.
        let cands = vec![
            (5, "Donkey Kong".to_string()),
            (
                284,
                "Advanced Dungeons & Dragons: Eye of the Beholder".to_string(),
            ),
            (284, "AD&D: Eye of the Beholder".to_string()),
            (300, "Eye of the Storm".to_string()),
        ];
        let result = match_title("ad-d---eye-of-the-beholder", &cands);
        assert!(result.is_some());
        assert_eq!(result.unwrap().game_id, 284);
    }

    #[test]
    fn threshold_behavior() {
        // A close but not exact match should still exceed threshold
        let result = match_title("Mega Man 5", &candidates());
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.game_id, 125);
        assert!(m.score >= SIMILARITY_THRESHOLD);
    }

    #[test]
    fn best_match_wins_among_similar() {
        let cands = vec![
            (1, "Mega Man".to_string()),
            (2, "Mega Man 2".to_string()),
            (3, "Mega Man 3".to_string()),
            (4, "Mega Man 4".to_string()),
            (5, "Mega Man 5".to_string()),
        ];
        let result = match_title("Mega Man 5", &cands);
        assert!(result.is_some());
        assert_eq!(result.unwrap().game_id, 5);
    }

    #[test]
    fn match_with_the_prefix() {
        let result = match_title("Legend of Zelda", &candidates());
        // "Legend of Zelda" vs "The Legend of Zelda" — should still match
        assert!(result.is_some());
        assert_eq!(result.unwrap().game_id, 113);
    }
}
