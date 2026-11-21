pub fn format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as i64;
    let (hours, minutes, remainder) = (total / 3_600, total % 3_600 / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{remainder:02}")
    } else {
        format!("{minutes}:{remainder:02}")
    }
}

pub fn format_timecode(seconds: f64) -> String {
    let hundredths = (seconds.max(0.0) * 100.0).round() as i64;
    format!(
        "{:02}:{:02}.{:02}",
        hundredths / 6_000,
        hundredths / 100 % 60,
        hundredths % 100
    )
}

pub fn format_size(bytes: u64) -> String {
    const MEGABYTE: f64 = 1_048_576.0;
    const GIGABYTE: f64 = 1_073_741_824.0;
    let bytes = bytes as f64;
    if bytes >= GIGABYTE {
        format!("{:.1} GB", bytes / GIGABYTE)
    } else if bytes >= 100.0 * MEGABYTE {
        format!("{:.0} MB", bytes / MEGABYTE)
    } else {
        format!("{:.1} MB", bytes / MEGABYTE)
    }
}

pub fn format_count(count: usize) -> String {
    let digits = count.to_string();
    let mut grouped = String::new();
    for (position, digit) in digits.chars().enumerate() {
        if position > 0 && (digits.len() - position).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_and_timecodes() {
        assert_eq!(format_duration(42.4), "0:42");
        assert_eq!(format_duration(3_725.0), "1:02:05");
        assert_eq!(format_timecode(18.42), "00:18.42");
        assert_eq!(format_timecode(125.5), "02:05.50");
    }

    #[test]
    fn sizes_and_counts() {
        assert_eq!(format_size(252 * 1_048_576), "252 MB");
        assert_eq!(format_size(1_288_490_189), "1.2 GB");
        assert_eq!(format_count(2_418), "2,418");
        assert_eq!(format_count(12), "12");
        assert_eq!(format_count(1_000_000), "1,000,000");
    }
}
