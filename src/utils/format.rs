/// Formátuje číslo s oddělovači pro lepší čitelnost
/// Příklad: 1234567 -> "1 234 567"
pub fn format_number(num: u64) -> String {
    let num_str = num.to_string();
    let mut result = String::new();
    let mut count = 0;

    for c in num_str.chars().rev() {
        if count > 0 && count % 3 == 0 {
            result.push(' ');
        }
        result.push(c);
        count += 1;
    }

    result.chars().rev().collect()
}

/// Formátuje velikost v bytech na human-readable formát
/// Příklad: 1024 -> "1.0 KB", 1048576 -> "1.0 MB"
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];

    if bytes == 0 {
        return "0 B".to_string();
    }

    let bytes_f = bytes as f64;
    let exp = (bytes_f.ln() / 1024_f64.ln()).floor() as usize;
    let exp = exp.min(UNITS.len() - 1);

    let value = bytes_f / 1024_f64.powi(exp as i32);

    // Pokud je hodnota větší než 100, zobraz bez desetinných míst
    if value >= 100.0 {
        format!("{:.0} {}", value, UNITS[exp])
    } else if value >= 10.0 {
        format!("{:.1} {}", value, UNITS[exp])
    } else {
        format!("{:.2} {}", value, UNITS[exp])
    }
}

/// Parsuje string s velikostí (např. "1.2gb") na bytes
pub fn parse_size_to_bytes(size: &str) -> u64 {
    let size = size.trim().to_lowercase();

    if size == "-" || size.is_empty() {
        return 0;
    }

    let mut num_str = String::new();
    let mut unit = String::new();

    for c in size.chars() {
        if c.is_numeric() || c == '.' {
            num_str.push(c);
        } else if c.is_alphabetic() {
            unit.push(c);
        }
    }

    let num: f64 = num_str.parse().unwrap_or(0.0);

    let multiplier: u64 = match unit.as_str() {
        "b" => 1,
        "kb" => 1024,
        "mb" => 1024 * 1024,
        "gb" => 1024 * 1024 * 1024,
        "tb" => 1024 * 1024 * 1024 * 1024,
        _ => 1,
    };

    (num * multiplier as f64) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(123), "123");
        assert_eq!(format_number(1234), "1 234");
        assert_eq!(format_number(1234567), "1 234 567");
        assert_eq!(format_number(1234567890), "1 234 567 890");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_parse_size_to_bytes() {
        assert_eq!(parse_size_to_bytes("1kb"), 1024);
        assert_eq!(parse_size_to_bytes("1.5mb"), 1572864);
        assert_eq!(parse_size_to_bytes("2gb"), 2147483648);
        assert_eq!(parse_size_to_bytes("-"), 0);
    }
}
