use std::time::{SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

const WINDOWS_EPOCH_OFFSET_SECONDS: u64 = 11_644_473_600;
const FILETIME_TICKS_PER_SECOND: u64 = 10_000_000;
const WEEKDAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const FULL_MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    pub fn today() -> Self {
        LocalDateTime::now().map_or(
            Self {
                year: 1970,
                month: 1,
                day: 1,
            },
            |now| now.date,
        )
    }

    pub fn days_since_epoch(self) -> i64 {
        let year = i64::from(self.year) - i64::from(self.month <= 2);
        let era = year.div_euclid(400);
        let year_of_era = year.rem_euclid(400);
        let month_index = i64::from((self.month + 9) % 12);
        let day_of_year = (153 * month_index + 2) / 5 + i64::from(self.day) - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }

    pub fn from_days_since_epoch(days: i64) -> Self {
        let shifted = days + 719_468;
        let era = shifted.div_euclid(146_097);
        let day_of_era = shifted.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_index = (5 * day_of_year + 2) / 153;
        let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
        let month = if month_index < 10 {
            month_index + 3
        } else {
            month_index - 9
        } as u32;
        let year = (year_of_era + era * 400) as i32 + i32::from(month <= 2);
        Self { year, month, day }
    }

    pub fn is_valid(self) -> bool {
        (1..=12).contains(&self.month)
            && self.day >= 1
            && Self::from_days_since_epoch(self.days_since_epoch()) == self
    }

    pub fn weekday_name(self) -> &'static str {
        let monday_based = (self.days_since_epoch() + 3).rem_euclid(7);
        WEEKDAY_NAMES[monday_based as usize]
    }

    pub fn short_label(self) -> String {
        format!(
            "{}, {} {}",
            self.weekday_name(),
            MONTH_NAMES[self.month as usize - 1],
            self.day
        )
    }

    pub fn month_label(self) -> String {
        format!(
            "{} {}",
            FULL_MONTH_NAMES[self.month as usize - 1],
            self.year
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalDateTime {
    pub date: Date,
    pub seconds_of_day: u32,
}

impl LocalDateTime {
    pub fn new(date: Date, hour: u32, minute: u32, second: u32) -> Option<Self> {
        (date.is_valid() && hour < 24 && minute < 60 && second < 60).then_some(Self {
            date,
            seconds_of_day: hour * 3_600 + minute * 60 + second,
        })
    }

    pub fn clock_label(self) -> String {
        format!(
            "{:02}:{:02}",
            self.seconds_of_day / 3_600,
            self.seconds_of_day % 3_600 / 60
        )
    }

    pub fn from_system_time(time: SystemTime) -> Option<Self> {
        let unix_seconds = time.duration_since(UNIX_EPOCH).ok()?.as_secs();
        let ticks = (unix_seconds + WINDOWS_EPOCH_OFFSET_SECONDS) * FILETIME_TICKS_PER_SECOND;
        let file_time = FILETIME {
            dwLowDateTime: ticks as u32,
            dwHighDateTime: (ticks >> 32) as u32,
        };
        let mut universal = SYSTEMTIME::default();
        let mut local = SYSTEMTIME::default();
        unsafe {
            FileTimeToSystemTime(&file_time, &mut universal).ok()?;
            SystemTimeToTzSpecificLocalTime(None, &universal, &mut local).ok()?;
        }
        Self::new(
            Date {
                year: i32::from(local.wYear),
                month: u32::from(local.wMonth),
                day: u32::from(local.wDay),
            },
            u32::from(local.wHour),
            u32::from(local.wMinute),
            u32::from(local.wSecond),
        )
    }

    pub fn now() -> Option<Self> {
        Self::from_system_time(SystemTime::now())
    }
}

pub fn days_between(earlier: Date, later: Date) -> i64 {
    later.days_since_epoch() - earlier.days_since_epoch()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_round_trip() {
        for days in [-1_000_000, -1, 0, 1, 19_000, 20_413, 2_932_896] {
            assert_eq!(Date::from_days_since_epoch(days).days_since_epoch(), days);
        }
        assert_eq!(
            Date {
                year: 1970,
                month: 1,
                day: 1
            }
            .days_since_epoch(),
            0
        );
    }

    #[test]
    fn weekday_of_known_dates() {
        assert_eq!(
            Date {
                year: 2026,
                month: 11,
                day: 21
            }
            .weekday_name(),
            "Sat"
        );
        assert_eq!(
            Date {
                year: 1970,
                month: 1,
                day: 1
            }
            .weekday_name(),
            "Thu"
        );
    }

    #[test]
    fn invalid_dates_are_rejected() {
        assert!(
            !Date {
                year: 2026,
                month: 2,
                day: 30
            }
            .is_valid()
        );
        assert!(
            !Date {
                year: 2026,
                month: 13,
                day: 1
            }
            .is_valid()
        );
        assert!(
            Date {
                year: 2024,
                month: 2,
                day: 29
            }
            .is_valid()
        );
    }

    #[test]
    fn labels_and_formats() {
        let date = Date {
            year: 2026,
            month: 11,
            day: 21,
        };
        assert_eq!(date.short_label(), "Sat, Nov 21");
        assert_eq!(date.month_label(), "November 2026");
        assert_eq!(
            LocalDateTime::new(date, 22, 41, 7).unwrap().clock_label(),
            "22:41"
        );
    }

    #[test]
    fn system_time_converts_to_a_plausible_local_time() {
        let now = LocalDateTime::now().unwrap();
        assert!(now.date.year >= 2026);
    }
}
