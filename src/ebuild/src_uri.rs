// src_uri.rs - SRC_URI parser with full variable expansion
//
// Parses SRC_URI with support for:
// - Variable expansion: ${PV}, ${PN}, ${P}, etc.
// - Command substitution: $(ver_cut 1-2), $(ver_rs 3 '-')
// - USE conditionals: use? ( uri )
// - Rename operator: uri -> filename

use std::collections::HashMap;
use crate::exception::InvalidData;
use super::version::{ver_cut, ver_rs};

/// Represents a parsed SRC_URI entry
#[derive(Debug, Clone)]
pub struct SrcUri {
    /// The URL to download from
    pub uri: String,
    /// Optional filename to save as (from -> operator)
    pub filename: Option<String>,
    /// USE flag condition (if any)
    pub use_conditional: Option<String>,
}

pub fn expand_string(input: &str, vars: &HashMap<String, String>) -> Result<String, InvalidData> {
    let mut result = input.to_string();
    
    result = expand_parameter_substitutions(&result, vars)?;
    
    result = expand_command_substitutions(&result, vars)?;
    
    Ok(result)
}

fn expand_parameter_substitutions(input: &str, vars: &HashMap<String, String>) -> Result<String, InvalidData> {
    let mut result = String::new();
    let mut chars = input.chars().peekable();
    
    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next();
            
            let mut var_expr = String::new();
            let mut depth = 1;
            
            while let Some(ch) = chars.next() {
                if ch == '{' {
                    depth += 1;
                    var_expr.push(ch);
                } else if ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    var_expr.push(ch);
                } else {
                    var_expr.push(ch);
                }
            }
            
            let expanded = expand_variable_expression(&var_expr, vars)?;
            result.push_str(&expanded);
        } else {
            result.push(ch);
        }
    }
    
    Ok(result)
}

fn expand_variable_expression(expr: &str, vars: &HashMap<String, String>) -> Result<String, InvalidData> {
    if expr.contains("%%") {
        let parts: Vec<&str> = expr.splitn(2, "%%").collect();
        let var_name = parts[0];
        let pattern = if parts.len() > 1 { parts[1] } else { "" };
        
        if let Some(value) = vars.get(var_name) {
            Ok(remove_suffix_greedy(value, pattern))
        } else {
            Ok(String::new())
        }
    } else if expr.contains("%") && !expr.contains("%%") {
        let parts: Vec<&str> = expr.splitn(2, '%').collect();
        let var_name = parts[0];
        let pattern = if parts.len() > 1 { parts[1] } else { "" };
        
        if let Some(value) = vars.get(var_name) {
            Ok(remove_suffix(value, pattern))
        } else {
            Ok(String::new())
        }
    } else if expr.contains("##") {
        let parts: Vec<&str> = expr.splitn(2, "##").collect();
        let var_name = parts[0];
        let pattern = if parts.len() > 1 { parts[1] } else { "" };
        
        if let Some(value) = vars.get(var_name) {
            Ok(remove_prefix_greedy(value, pattern))
        } else {
            Ok(String::new())
        }
    } else if expr.contains("#") && !expr.contains("##") {
        let parts: Vec<&str> = expr.splitn(2, '#').collect();
        let var_name = parts[0];
        let pattern = if parts.len() > 1 { parts[1] } else { "" };
        
        if let Some(value) = vars.get(var_name) {
            Ok(remove_prefix(value, pattern))
        } else {
            Ok(String::new())
        }
    } else if expr.contains("/") {
        if let Some(slash_pos) = expr.find('/') {
            let var_name = &expr[..slash_pos];
            let rest = &expr[slash_pos + 1..];
            
            if let Some(value) = vars.get(var_name) {
                if let Some(second_slash) = rest.find('/') {
                    let pattern = &rest[..second_slash];
                    let replacement = &rest[second_slash + 1..];
                    
                    if rest.starts_with('/') {
                        Ok(value.replace(pattern, replacement))
                    } else {
                        Ok(value.replacen(pattern, replacement, 1))
                    }
                } else {
                    Ok(value.clone())
                }
            } else {
                Ok(String::new())
            }
        } else {
            Ok(vars.get(expr).cloned().unwrap_or_default())
        }
    } else if expr.contains(":+") {
        let parts: Vec<&str> = expr.splitn(2, ":+").collect();
        let var_name = parts[0];
        let alt_value = if parts.len() > 1 { parts[1] } else { "" };
        
        if vars.contains_key(var_name) && !vars.get(var_name).unwrap().is_empty() {
            Ok(alt_value.to_string())
        } else {
            Ok(String::new())
        }
    } else if expr.contains(":-") {
        let parts: Vec<&str> = expr.splitn(2, ":-").collect();
        let var_name = parts[0];
        let default = if parts.len() > 1 { parts[1] } else { "" };
        
        if let Some(value) = vars.get(var_name) {
            if !value.is_empty() {
                Ok(value.clone())
            } else {
                Ok(default.to_string())
            }
        } else {
            Ok(default.to_string())
        }
    } else {
        Ok(vars.get(expr).cloned().unwrap_or_default())
    }
}

fn remove_suffix_greedy(s: &str, pattern: &str) -> String {
    let mut result = s.to_string();
    while result.ends_with(pattern) || pattern.contains('*') {
        if pattern == "*" {
            return String::new();
        }
        
        if let Some(star_pos) = pattern.rfind('*') {
            let suffix = &pattern[star_pos + 1..];
            if let Some(pos) = result.rfind(suffix) {
                result.truncate(pos);
                return result;
            }
            return result;
        }
        
        if result.ends_with(pattern) {
            result.truncate(result.len() - pattern.len());
            return result;
        }
        break;
    }
    result
}

fn remove_suffix(s: &str, pattern: &str) -> String {
    if s.ends_with(pattern) {
        s[..s.len() - pattern.len()].to_string()
    } else {
        s.to_string()
    }
}

fn remove_prefix_greedy(s: &str, pattern: &str) -> String {
    let mut result = s.to_string();
    while result.starts_with(pattern) || pattern.contains('*') {
        if pattern == "*" {
            return String::new();
        }
        
        if let Some(star_pos) = pattern.find('*') {
            let prefix = &pattern[..star_pos];
            if let Some(pos) = result.find(prefix) {
                result = result[pos + prefix.len()..].to_string();
                return result;
            }
            return result;
        }
        
        if result.starts_with(pattern) {
            result = result[pattern.len()..].to_string();
            return result;
        }
        break;
    }
    result
}

fn remove_prefix(s: &str, pattern: &str) -> String {
    if s.starts_with(pattern) {
        s[pattern.len()..].to_string()
    } else {
        s.to_string()
    }
}

/// Expand command substitutions like $(ver_cut 1-2) and $(ver_rs 3 '-')
fn expand_command_substitutions(input: &str, vars: &HashMap<String, String>) -> Result<String, InvalidData> {
    let mut result = String::new();
    let mut chars = input.chars().peekable();
    
    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'(') {
            chars.next(); // consume '('
            
            // Extract the command
            let mut command = String::new();
            let mut depth = 1;
            
            while let Some(ch) = chars.next() {
                if ch == '(' {
                    depth += 1;
                    command.push(ch);
                } else if ch == ')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    command.push(ch);
                } else {
                    command.push(ch);
                }
            }
            
            // Execute the command
            let expanded = execute_command(&command, vars)?;
            result.push_str(&expanded);
        } else {
            result.push(ch);
        }
    }
    
    Ok(result)
}

fn tokenize_command(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = ' ';
    
    for ch in command.chars() {
        match ch {
            '"' | '\'' if !in_quote => {
                in_quote = true;
                quote_char = ch;
            }
            c if c == quote_char && in_quote => {
                in_quote = false;
            }
            ' ' | '\t' if !in_quote => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            c => current.push(c),
        }
    }
    
    if !current.is_empty() {
        tokens.push(current);
    }
    
    tokens
}

fn execute_command(command: &str, vars: &HashMap<String, String>) -> Result<String, InvalidData> {
    let parts = tokenize_command(command);
    
    if parts.is_empty() {
        return Ok(String::new());
    }
    
    let cmd = &parts[0];
    let args = &parts[1..];
    
    match cmd.as_str() {
        "ver_cut" => {
            if args.is_empty() {
                return Err(InvalidData::new("ver_cut requires at least 1 argument", None));
            }
            let range = &args[0];
            let version = if args.len() > 1 {
                let mut v = args[1].to_string();
                for (var, value) in vars {
                    v = v.replace(&format!("${{{}}}", var), value);
                    v = v.replace(&format!("${}", var), value);
                }
                v.trim_matches('"').trim_matches('\'').to_string()
            } else {
                vars.get("PV").ok_or_else(|| InvalidData::new("PV not set for ver_cut", None))?.clone()
            };
            
            ver_cut(range, &version)
                .map_err(|e| InvalidData::new(&format!("ver_cut failed: {}", e), None))
        }
        "ver_rs" => {
            if args.len() < 2 {
                return Err(InvalidData::new("ver_rs requires at least 2 arguments", None));
            }
            let range = &args[0];
            let replacement = args[1].trim_matches('"').trim_matches('\'');
            let version = if args.len() > 2 {
                let mut v = args[2].to_string();
                for (var, value) in vars {
                    v = v.replace(&format!("${{{}}}", var), value);
                    v = v.replace(&format!("${}", var), value);
                }
                v.trim_matches('"').trim_matches('\'').to_string()
            } else {
                vars.get("PV").ok_or_else(|| InvalidData::new("PV not set for ver_rs", None))?.clone()
            };
            
            ver_rs(range, replacement, &version)
                .map_err(|e| InvalidData::new(&format!("ver_rs failed: {}", e), None))
        }
        _ => {
            Err(InvalidData::new(&format!("Unknown command substitution: {}", cmd), None))
        }
    }
}

/// Parse SRC_URI string into structured URIs
pub fn parse_src_uri(src_uri: &str, vars: &HashMap<String, String>, use_flags: &HashMap<String, bool>) -> Result<Vec<SrcUri>, InvalidData> {
    let mut uris = Vec::new();
    let tokens = tokenize_src_uri(src_uri);
    let mut i = 0;
    
    while i < tokens.len() {
        let token = &tokens[i];
        
        // Check for USE conditional
        if token.ends_with('?') {
            let use_flag = token.trim_end_matches('?');
            let enabled = use_flags.get(use_flag).copied().unwrap_or(false);
            
            // Skip to opening parenthesis
            i += 1;
            if i >= tokens.len() || tokens[i] != "(" {
                return Err(InvalidData::new("Expected '(' after USE conditional", None));
            }
            i += 1;
            
            // Collect tokens until closing parenthesis
            let mut depth = 1;
            let mut conditional_tokens = Vec::new();
            
            while i < tokens.len() && depth > 0 {
                if tokens[i] == "(" {
                    depth += 1;
                    conditional_tokens.push(tokens[i].clone());
                } else if tokens[i] == ")" {
                    depth -= 1;
                    if depth > 0 {
                        conditional_tokens.push(tokens[i].clone());
                    }
                } else {
                    conditional_tokens.push(tokens[i].clone());
                }
                i += 1;
            }
            
            // Only process if USE flag is enabled
            if enabled {
                let mut j = 0;
                while j < conditional_tokens.len() {
                    if let Some(uri) = parse_uri_entry(&conditional_tokens, &mut j, vars)? {
                        uris.push(SrcUri {
                            uri: uri.0,
                            filename: uri.1,
                            use_conditional: Some(use_flag.to_string()),
                        });
                    }
                }
            }
        } else if token == "(" || token == ")" {
            i += 1;
        } else if let Some(uri) = parse_uri_entry(&tokens, &mut i, vars)? {
            uris.push(SrcUri {
                uri: uri.0,
                filename: uri.1,
                use_conditional: None,
            });
        }
    }
    
    Ok(uris)
}

/// Parse a single URI entry (with optional -> filename)
fn parse_uri_entry(tokens: &[String], index: &mut usize, vars: &HashMap<String, String>) -> Result<Option<(String, Option<String>)>, InvalidData> {
    if *index >= tokens.len() {
        return Ok(None);
    }
    
    let uri = expand_string(&tokens[*index], vars)?;
    *index += 1;
    
    // Skip if this is a control token
    if uri == "(" || uri == ")" || uri.ends_with('?') {
        return Ok(None);
    }
    
    // Check for -> operator
    if *index < tokens.len() && tokens[*index] == "->" {
        *index += 1;
        if *index < tokens.len() {
            let filename = expand_string(&tokens[*index], vars)?;
            *index += 1;
            return Ok(Some((uri, Some(filename))));
        } else {
            return Err(InvalidData::new("Expected filename after '->'", None));
        }
    }
    
    Ok(Some((uri, None)))
}

/// Tokenize SRC_URI string (split on whitespace but preserve quoted strings)
fn tokenize_src_uri(src_uri: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = ' ';
    
    for ch in src_uri.chars() {
        match ch {
            '"' | '\'' if !in_quote => {
                in_quote = true;
                quote_char = ch;
            }
            c if c == quote_char && in_quote => {
                in_quote = false;
            }
            ' ' | '\t' | '\n' if !in_quote => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            c => current.push(c),
        }
    }
    
    if !current.is_empty() {
        tokens.push(current);
    }
    
    tokens
}

/// Get the filename for a URI (either from -> operator or extracted from URL)
pub fn get_filename(uri: &SrcUri) -> String {
    if let Some(ref filename) = uri.filename {
        return filename.clone();
    }
    
    // Extract from URI
    uri.uri
        .split('/')
        .last()
        .and_then(|s| s.split('?').next())
        .and_then(|s| s.split('#').next())
        .unwrap_or("archive")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_expand_string() {
        let mut vars = HashMap::new();
        vars.insert("PN".to_string(), "mypackage".to_string());
        vars.insert("PV".to_string(), "1.2.3".to_string());
        
        let result = expand_string("${PN}-${PV}.tar.gz", &vars).unwrap();
        assert_eq!(result, "mypackage-1.2.3.tar.gz");
    }
    
    #[test]
    fn test_ver_cut_substitution() {
        let mut vars = HashMap::new();
        vars.insert("PV".to_string(), "1.2.3".to_string());
        
        let result = expand_string("version-$(ver_cut 1-2).tar.gz", &vars).unwrap();
        assert_eq!(result, "version-1.2.tar.gz");
    }
    
    #[test]
    fn test_ver_rs_substitution() {
        let mut vars = HashMap::new();
        vars.insert("PV".to_string(), "1.2.3".to_string());
        
        let result = expand_string("package-$(ver_rs 2 _).tar.gz", &vars).unwrap();
        assert_eq!(result, "package-1.2_3.tar.gz");
    }
    
    #[test]
    fn test_parse_src_uri() {
        let mut vars = HashMap::new();
        vars.insert("PN".to_string(), "mypackage".to_string());
        vars.insert("PV".to_string(), "1.2.3".to_string());
        
        let use_flags = HashMap::new();
        
        let uris = parse_src_uri("https://example.com/${PN}-${PV}.tar.gz", &vars, &use_flags).unwrap();
        assert_eq!(uris.len(), 1);
        assert_eq!(uris[0].uri, "https://example.com/mypackage-1.2.3.tar.gz");
    }
    
    #[test]
    fn test_parse_src_uri_with_rename() {
        let mut vars = HashMap::new();
        vars.insert("PN".to_string(), "mypackage".to_string());
        
        let use_flags = HashMap::new();
        
        let uris = parse_src_uri("https://example.com/archive.tar.gz -> ${PN}.tar.gz", &vars, &use_flags).unwrap();
        assert_eq!(uris.len(), 1);
        assert_eq!(uris[0].uri, "https://example.com/archive.tar.gz");
        assert_eq!(uris[0].filename, Some("mypackage.tar.gz".to_string()));
    }
}
