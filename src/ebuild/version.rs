// version.rs - Version manipulation helpers (EAPI 9 version functions)
//
// Implements ver_cut, ver_rs, and other version manipulation functions

/// Extract a substring from a version string based on component ranges
/// 
/// ver_cut <begin>[-<end>] [<version>]
/// Examples:
/// ver_cut 1 1.2.3 => 1
/// ver_cut 1-2 1.2.3 => 1.2
/// ver_cut 2- 1.2.3 => 2.3
pub fn ver_cut(range: &str, version: &str) -> Result<String, String> {
    let components: Vec<&str> = version.split('.').collect();
    
    let (begin, end) = parse_range(range, components.len())?;
    
    if begin > components.len() || begin < 1 {
        return Err(format!("Begin index {} out of range for version {}", begin, version));
    }
    
    let end = end.min(components.len());
    
    Ok(components[(begin - 1)..end].join("."))
}

/// Replace separators in version string
///
/// ver_rs <begin>[-<end>] <replacement> [<version>]
/// Examples:
/// ver_rs 1 - 1.2.3 => 1-2.3
/// ver_rs 2 _ 1.2.3 => 1.2_3
/// ver_rs 1-2 - 1.2.3 => 1-2-3
pub fn ver_rs(range: &str, replacement: &str, version: &str) -> Result<String, String> {
    let components: Vec<String> = version.split('.').map(|s| s.to_string()).collect();
    
    let (begin, end) = parse_range(range, components.len())?;
    
    if begin > components.len() || begin < 1 {
        return Err(format!("Begin index {} out of range for version {}", begin, version));
    }
    
    let end_idx = end.min(components.len());
    
    let mut result = components[0].clone();
    
    for i in 1..components.len() {
        let separator = if i >= begin && i <= end_idx {
            replacement
        } else {
            "."
        };
        result.push_str(separator);
        result.push_str(&components[i]);
    }
    
    Ok(result)
}

/// Parse a range string like "1", "1-2", "2-", etc.
fn parse_range(range: &str, max_len: usize) -> Result<(usize, usize), String> {
    if range.contains('-') {
        let parts: Vec<&str> = range.split('-').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid range: {}", range));
        }
        
        let begin = if parts[0].is_empty() {
            1
        } else {
            parts[0].parse::<usize>()
                .map_err(|_| format!("Invalid begin index: {}", parts[0]))?
        };
        
        let end = if parts[1].is_empty() {
            max_len
        } else {
            parts[1].parse::<usize>()
                .map_err(|_| format!("Invalid end index: {}", parts[1]))?
        };
        
        Ok((begin, end))
    } else {
        let index = range.parse::<usize>()
            .map_err(|_| format!("Invalid index: {}", range))?;
        Ok((index, index))
    }
}

/// Compare two version strings
/// Returns: -1 if v1 < v2, 0 if v1 == v2, 1 if v1 > v2
pub fn ver_compare(v1: &str, v2: &str) -> i32 {
    let parts1 = split_version(v1);
    let parts2 = split_version(v2);
    
    for i in 0..parts1.len().max(parts2.len()) {
        let p1 = parts1.get(i).map(|s| s.as_str()).unwrap_or("");
        let p2 = parts2.get(i).map(|s| s.as_str()).unwrap_or("");
        
        match compare_component(p1, p2) {
            0 => continue,
            n => return n,
        }
    }
    
    0
}

fn split_version(version: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut last_was_digit = false;
    
    for ch in version.chars() {
        let is_digit = ch.is_ascii_digit();
        
        if !current.is_empty() && is_digit != last_was_digit {
            parts.push(current.clone());
            current.clear();
        }
        
        current.push(ch);
        last_was_digit = is_digit;
    }
    
    if !current.is_empty() {
        parts.push(current);
    }
    
    parts
}

fn compare_component(c1: &str, c2: &str) -> i32 {
    if c1.is_empty() && c2.is_empty() {
        return 0;
    }
    if c1.is_empty() {
        return -1;
    }
    if c2.is_empty() {
        return 1;
    }
    
    let n1 = c1.parse::<i64>();
    let n2 = c2.parse::<i64>();
    
    match (n1, n2) {
        (Ok(n1), Ok(n2)) => {
            if n1 < n2 { -1 } else if n1 > n2 { 1 } else { 0 }
        }
        _ => {
            if c1 < c2 { -1 } else if c1 > c2 { 1 } else { 0 }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ver_cut() {
        assert_eq!(ver_cut("1", "1.2.3").unwrap(), "1");
        assert_eq!(ver_cut("1-2", "1.2.3").unwrap(), "1.2");
        assert_eq!(ver_cut("2-", "1.2.3").unwrap(), "2.3");
        assert_eq!(ver_cut("2", "1.2.3").unwrap(), "2");
    }
    
    #[test]
    fn test_ver_rs() {
        assert_eq!(ver_rs("1", "-", "1.2.3").unwrap(), "1-2.3");
        assert_eq!(ver_rs("2", "_", "1.2.3").unwrap(), "1.2_3");
        assert_eq!(ver_rs("1-2", "-", "1.2.3").unwrap(), "1-2-3");
    }
    
    #[test]
    fn test_ver_compare() {
        assert_eq!(ver_compare("1.2.3", "1.2.3"), 0);
        assert_eq!(ver_compare("1.2.3", "1.2.4"), -1);
        assert_eq!(ver_compare("1.2.4", "1.2.3"), 1);
        assert_eq!(ver_compare("1.10.0", "1.9.0"), 1);
    }
}
