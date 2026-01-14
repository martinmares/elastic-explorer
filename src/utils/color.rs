use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Generuje konzistentní HSL barvu z názvu indexu
pub fn generate_index_color(index_name: &str) -> String {
    let mut hasher = DefaultHasher::new();
    index_name.hash(&mut hasher);
    let hash = hasher.finish();

    // Použij hash pro generování Hue (0-360)
    let hue = (hash % 360) as u32;

    // Saturace 70% a světlost 40% pro dobrý kontrast
    format!("hsl({}, 70%, 40%)", hue)
}

/// Rozhodne zda použít bílý nebo černý text podle barvy pozadí
/// Používá správný výpočet relativní luminance podle WCAG
pub fn get_text_color_for_background(bg_color: &str) -> String {
    if let Some((h, s, l)) = parse_hsl(bg_color) {
        // Převeď HSL na RGB
        let (r, g, b) = hsl_to_rgb(h, s, l);

        // Vypočítej relativní luminanci podle WCAG
        let luminance = calculate_relative_luminance(r, g, b);

        // Debug log pro diagnostiku
        // tracing::debug!("Color HSL({:.0}, {:.0}%, {:.0}%) -> RGB({:.2}, {:.2}, {:.2}) -> Luminance: {:.3}",
        //    h, s * 100.0, l * 100.0, r, g, b, luminance);

        // Pro žlutou a světle zelenou: nižší práh (0.35)
        // Pro ostatní: vyšší práh (0.5)
        let threshold = if (h >= 40.0 && h <= 80.0) || (h >= 90.0 && h <= 150.0) {
            0.35  // Žlutá a světle zelená oblast
        } else {
            0.5   // Ostatní barvy
        };

        if luminance > threshold {
            "black".to_string()
        } else {
            "white".to_string()
        }
    } else {
        // Default: bílý text
        "white".to_string()
    }
}

/// Parsuje HSL string a vrací (hue, saturation, lightness)
fn parse_hsl(hsl: &str) -> Option<(f32, f32, f32)> {
    // Formát: "hsl(120, 70%, 40%)"
    let parts: Vec<&str> = hsl.trim_start_matches("hsl(")
        .trim_end_matches(')')
        .split(',')
        .collect();

    if parts.len() == 3 {
        let h = parts[0].trim().parse::<f32>().ok()?;
        let s = parts[1].trim().trim_end_matches('%').parse::<f32>().ok()? / 100.0;
        let l = parts[2].trim().trim_end_matches('%').parse::<f32>().ok()? / 100.0;
        Some((h, s, l))
    } else {
        None
    }
}

/// Převede HSL na RGB (hodnoty 0.0-1.0)
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());

    let (r1, g1, b1) = match h_prime as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        5 => (c, 0.0, x),
        _ => (c, 0.0, x),
    };

    let m = l - c / 2.0;
    (r1 + m, g1 + m, b1 + m)
}

/// Vypočítá relativní luminanci podle WCAG 2.0
fn calculate_relative_luminance(r: f32, g: f32, b: f32) -> f32 {
    // Linearizuj RGB hodnoty
    let r_lin = if r <= 0.03928 { r / 12.92 } else { ((r + 0.055) / 1.055).powf(2.4) };
    let g_lin = if g <= 0.03928 { g / 12.92 } else { ((g + 0.055) / 1.055).powf(2.4) };
    let b_lin = if b <= 0.03928 { b / 12.92 } else { ((b + 0.055) / 1.055).powf(2.4) };

    // WCAG formula pro relativní luminanci
    0.2126 * r_lin + 0.7152 * g_lin + 0.0722 * b_lin
}

/// Vrací barvu podle stavu shardu (pro border)
pub fn shard_state_color(state: &str) -> String {
    match state {
        "STARTED" => "#2fb344".to_string(), // zelená
        "RELOCATING" => "#f59f00".to_string(), // oranžová
        "INITIALIZING" => "#206bc4".to_string(), // modrá
        "UNASSIGNED" => "#d63939".to_string(), // červená
        _ => "#626976".to_string(), // šedá pro neznámé stavy
    }
}
