use std::time::Duration;

const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

pub fn format_bytes(bytes: u64) -> String {
    format_units(bytes, "")
}

pub fn format_rate(bytes_per_second: u64) -> String {
    format_units(bytes_per_second, "/s")
}

fn format_units(value: u64, suffix: &str) -> String {
    if value < 1024 {
        return format!("{value} B{suffix}");
    }

    let mut scaled = value as f64;
    let mut unit = 0;
    while scaled >= 1024.0 && unit < UNITS.len() - 1 {
        scaled /= 1024.0;
        unit += 1;
    }
    format!("{scaled:.1} {}{suffix}", UNITS[unit])
}

/// btop-style compact size for narrow table columns: at most 5 chars with a
/// single-letter unit ("512B", "3.9M", "568M", "1.2G"). One decimal only
/// below 10 so the unit is never truncated away.
pub fn format_bytes_compact(bytes: u64) -> String {
    const SHORT: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    let mut scaled = bytes as f64;
    let mut unit = 0;
    // Promote while the integer part would need more than 4 digits.
    while scaled >= 10_000.0 && unit < SHORT.len() - 1 {
        scaled /= 1024.0;
        unit += 1;
    }
    // Prefer the next unit once it reads naturally (e.g. 1200M -> 1.2G).
    if scaled >= 1024.0 && unit < SHORT.len() - 1 {
        scaled /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{}B", scaled as u64)
    } else if scaled < 10.0 {
        format!("{scaled:.1}{}", SHORT[unit])
    } else {
        format!("{}{}", scaled as u64, SHORT[unit])
    }
}

pub fn format_duration(duration: Duration) -> String {
    let mut seconds = duration.as_secs();
    if seconds == 0 {
        return "0s".to_owned();
    }

    let days = seconds / 86_400;
    seconds %= 86_400;
    let hours = seconds / 3_600;
    seconds %= 3_600;
    let minutes = seconds / 60;
    seconds %= 60;

    let mut parts = Vec::with_capacity(4);
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if seconds > 0 {
        parts.push(format!("{seconds}s"));
    }
    parts.join(" ")
}

pub fn clamp_percent(value: f32) -> f64 {
    if value.is_nan() || value <= 0.0 {
        0.0
    } else if value >= 100.0 {
        100.0
    } else {
        value as f64
    }
}

pub fn format_percent(value: f32) -> String {
    format!("{:.1}%", clamp_percent(value))
}

pub fn truncate_text(text: &str, max_width: usize) -> String {
    if text.chars().count() <= max_width {
        return text.to_owned();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_owned();
    }

    let mut result: String = text.chars().take(max_width - 1).collect();
    result.push('…');
    result
}
