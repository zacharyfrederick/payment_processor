/// Normalizes CSV output for comparison: trims whitespace, normalizes line endings,
/// canonicalizes decimal formatting (4 dp), and sorts data rows by client id.
pub fn normalize_accounts_csv(s: &str) -> String {
    let normalized = s.trim().replace("\r\n", "\n");
    let lines: Vec<&str> = normalized
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();

    if lines.is_empty() {
        return String::new();
    }

    let header = lines[0]
        .split(',')
        .map(|f| f.trim())
        .collect::<Vec<_>>()
        .join(",");

    if lines.len() == 1 {
        return header;
    }

    let mut rows: Vec<Vec<String>> = lines[1..]
        .iter()
        .map(|line| {
            line.split(',')
                .map(|f| f.trim().to_string())
                .collect::<Vec<_>>()
        })
        .collect();

    // Canonicalize numeric columns (index 1=available, 2=held, 3=total)
    for row in &mut rows {
        if row.len() >= 4 {
            for cell in row.iter_mut().skip(1).take(3) {
                if let Ok(n) = cell.parse::<f64>() {
                    *cell = format!("{:.4}", n);
                }
            }
        }
    }

    // Sort by client (first column)
    rows.sort_by(|a, b| {
        let a_id = a.first().map(String::as_str).unwrap_or("");
        let b_id = b.first().map(String::as_str).unwrap_or("");
        a_id.cmp(b_id)
    });

    let data: String = rows
        .iter()
        .map(|r| r.join(","))
        .collect::<Vec<_>>()
        .join("\n");

    if data.is_empty() {
        header
    } else {
        format!("{}\n{}", header, data)
    }
}
