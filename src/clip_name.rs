use crate::local_time::{Date, LocalDateTime};

#[derive(Debug, PartialEq, Eq)]
pub struct ParsedName {
    pub title: String,
    pub taken: Option<LocalDateTime>,
}

pub fn parse(file_stem: &str) -> ParsedName {
    let Some((date_start, date)) = find_date(file_stem) else {
        return ParsedName {
            title: clean_title(file_stem),
            taken: None,
        };
    };
    let after_date = &file_stem[date_start + 10..];
    let taken = parse_time(after_date)
        .and_then(|(hour, minute, second)| LocalDateTime::new(date, hour, minute, second));
    ParsedName {
        title: clean_title(&file_stem[..date_start]),
        taken,
    }
}

fn clean_title(raw: &str) -> String {
    let trimmed = raw.trim_matches(|character: char| {
        character == '_' || character == '-' || character.is_whitespace()
    });
    let without_suffix = trimmed
        .strip_suffix("_replay")
        .or_else(|| trimmed.strip_suffix(" replay"))
        .unwrap_or(trimmed);
    let title = without_suffix.trim_matches(|character: char| {
        character == '_' || character == '-' || character.is_whitespace()
    });
    if title.is_empty() {
        raw.trim().to_string()
    } else {
        title.to_string()
    }
}

fn find_date(text: &str) -> Option<(usize, Date)> {
    let bytes = text.as_bytes();
    (0..bytes.len().saturating_sub(9)).find_map(|start| {
        let candidate = &bytes[start..start + 10];
        let shape_matches = candidate
            .iter()
            .enumerate()
            .all(|(index, byte)| match index {
                4 | 7 => *byte == b'.' || *byte == b'-',
                _ => byte.is_ascii_digit(),
            });
        if !shape_matches || (start > 0 && bytes[start - 1].is_ascii_digit()) {
            return None;
        }
        let date = Date {
            year: text[start..start + 4].parse().ok()?,
            month: text[start + 5..start + 7].parse().ok()?,
            day: text[start + 8..start + 10].parse().ok()?,
        };
        date.is_valid().then_some((start, date))
    })
}

fn parse_time(after_date: &str) -> Option<(u32, u32, u32)> {
    let groups: Vec<&str> = after_date
        .split(|character: char| !character.is_ascii_digit())
        .filter(|group| !group.is_empty())
        .collect();
    let first = *groups.first()?;
    if first.len() >= 6 {
        return Some((
            first[0..2].parse().ok()?,
            first[2..4].parse().ok()?,
            first[4..6].parse().ok()?,
        ));
    }
    let hour = first.parse().ok()?;
    let minute = groups.get(1)?.parse().ok()?;
    let second = groups
        .get(2)
        .and_then(|group| group.parse().ok())
        .unwrap_or(0);
    Some((hour, minute, second))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taken(parsed: &ParsedName) -> (i32, u32, u32, u32) {
        let moment = parsed.taken.expect("timestamp parsed");
        (
            moment.date.year,
            moment.date.month,
            moment.date.day,
            moment.seconds_of_day,
        )
    }

    #[test]
    fn radeon_relive_replay() {
        let parsed = parse("Counter-Strike 2_replay_2026.11.21-19.01");
        assert_eq!(parsed.title, "Counter-Strike 2");
        assert_eq!(taken(&parsed), (2026, 11, 21, 19 * 3_600 + 60));
    }

    #[test]
    fn nvidia_shadowplay() {
        let parsed = parse("Neon Drift 2026.11.21 - 22.41.07.05.DVR");
        assert_eq!(parsed.title, "Neon Drift");
        assert_eq!(taken(&parsed), (2026, 11, 21, 22 * 3_600 + 41 * 60 + 7));
    }

    #[test]
    fn xbox_game_bar_and_snipping_tool() {
        let game_bar = parse("Hollow Spire 2026-11-21 20-55-03");
        assert_eq!(game_bar.title, "Hollow Spire");
        assert_eq!(taken(&game_bar), (2026, 11, 21, 20 * 3_600 + 55 * 60 + 3));
        let recording = parse("Screen Recording 2026-10-26 155657");
        assert_eq!(recording.title, "Screen Recording");
        assert_eq!(taken(&recording), (2026, 10, 26, 15 * 3_600 + 56 * 60 + 57));
    }

    #[test]
    fn name_without_date_keeps_its_stem() {
        let parsed = parse("holiday_clip");
        assert_eq!(parsed.title, "holiday_clip");
        assert!(parsed.taken.is_none());
    }

    #[test]
    fn invalid_date_is_ignored() {
        assert!(parse("Clip 2026.13.40-10.00").taken.is_none());
    }
}
