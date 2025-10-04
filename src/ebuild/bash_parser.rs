// bash_parser.rs - Parse bash ebuild functions and translate to Rust calls
//
// This module parses bash function bodies from ebuilds and translates
// common helper calls into native Rust function invocations

use std::collections::HashMap;
use crate::exception::InvalidData;
use super::environment::EbuildEnvironment;

/// Represents a parsed bash command
#[derive(Debug, Clone)]
pub enum BashCommand {
    /// Helper function call (e.g., einfo "message")
    HelperCall {
        name: String,
        args: Vec<String>,
    },
    /// Variable assignment
    Assignment {
        name: String,
        value: String,
    },
    /// Conditional (if/case/etc)
    Conditional {
        condition: String,
        then_block: Vec<BashCommand>,
        else_block: Option<Vec<BashCommand>>,
    },
    /// Loop (for/while)
    Loop {
        variable: String,
        items: Vec<String>,
        body: Vec<BashCommand>,
    },
    /// Command substitution $(...)
    CommandSubstitution {
        command: String,
    },
    /// Raw bash code (fallback)
    RawBash {
        code: String,
    },
}

/// Parse a bash function body into commands
pub fn parse_bash_function(body: &str) -> Result<Vec<BashCommand>, InvalidData> {
    let mut commands = Vec::new();
    
    for line in body.lines() {
        let line = line.trim();
        
        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        
        // Try to parse as a known helper call
        if let Some(cmd) = parse_helper_call(line) {
            commands.push(cmd);
            continue;
        }
        
        // Try to parse as assignment
        if let Some(cmd) = parse_assignment(line) {
            commands.push(cmd);
            continue;
        }
        
        // Otherwise, treat as raw bash
        commands.push(BashCommand::RawBash {
            code: line.to_string(),
        });
    }
    
    Ok(commands)
}

/// Parse a helper function call
fn parse_helper_call(line: &str) -> Option<BashCommand> {
    // List of known helper functions
    const HELPERS: &[&str] = &[
        "einfo", "ewarn", "eerror", "die", "ebegin", "eend",
        "dobin", "doins", "doman", "dodoc", "dodir", "dosym",
        "emake", "econf", "eapply", "epatch",
        "tc-export", "tc-getCC", "tc-getCXX",
        "use", "usex", "use_enable", "use_with",
        "meson_use", "meson_feature",
        "cmake_src_configure", "meson_src_configure",
        "default", "default_src_prepare", "default_src_unpack",
    ];
    
    for helper in HELPERS {
        if line.starts_with(helper) {
            let rest = &line[helper.len()..].trim();
            let args = parse_args(rest);
            return Some(BashCommand::HelperCall {
                name: helper.to_string(),
                args,
            });
        }
    }
    
    None
}

/// Parse function arguments (simplified - handles quoted strings)
fn parse_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = ' ';
    
    for ch in input.chars() {
        match ch {
            '"' | '\'' if !in_quote => {
                in_quote = true;
                quote_char = ch;
            }
            c if c == quote_char && in_quote => {
                in_quote = false;
                if !current.is_empty() {
                    args.push(current.clone());
                    current.clear();
                }
            }
            ' ' if !in_quote => {
                if !current.is_empty() {
                    args.push(current.clone());
                    current.clear();
                }
            }
            c => current.push(c),
        }
    }
    
    if !current.is_empty() {
        args.push(current);
    }
    
    args
}

/// Parse variable assignment
fn parse_assignment(line: &str) -> Option<BashCommand> {
    if let Some(eq_pos) = line.find('=') {
        let name = line[..eq_pos].trim();
        let value = line[eq_pos + 1..].trim();
        
        // Only parse simple variable assignments (not function definitions)
        if !name.contains(' ') && !name.contains('(') {
            return Some(BashCommand::Assignment {
                name: name.to_string(),
                value: value.trim_matches('"').trim_matches('\'').to_string(),
            });
        }
    }
    
    None
}

/// Execute parsed bash commands using native Rust implementations
pub fn execute_bash_commands(
    commands: &[BashCommand],
    env: &mut EbuildEnvironment,
) -> Result<(), InvalidData> {
    for cmd in commands {
        execute_bash_command(cmd, env)?;
    }
    Ok(())
}

/// Execute a single bash command
fn execute_bash_command(
    cmd: &BashCommand,
    env: &mut EbuildEnvironment,
) -> Result<(), InvalidData> {
    match cmd {
        BashCommand::HelperCall { name, args } => {
            execute_helper_call(name, args, env)?;
        }
        BashCommand::Assignment { name, value } => {
            env.set(name.clone(), value.clone());
        }
        BashCommand::Conditional { .. } => {
            // TODO: Handle conditionals
            return Err(InvalidData::new("Conditionals not yet supported in native execution", None));
        }
        BashCommand::Loop { .. } => {
            // TODO: Handle loops
            return Err(InvalidData::new("Loops not yet supported in native execution", None));
        }
        BashCommand::CommandSubstitution { .. } => {
            // TODO: Handle command substitution
            return Err(InvalidData::new("Command substitution not yet supported in native execution", None));
        }
        BashCommand::RawBash { .. } => {
            // Can't execute raw bash natively - caller must use bash fallback
            return Err(InvalidData::new("Raw bash code requires bash fallback", None));
        }
    }
    Ok(())
}

/// Execute a helper function call using native Rust implementation
fn execute_helper_call(
    name: &str,
    args: &[String],
    env: &mut EbuildEnvironment,
) -> Result<(), InvalidData> {
    use super::helpers::{einfo, ewarn, eerror, ebegin, eend, default_src_prepare, default_src_unpack};
    use super::install_helpers::{dobin, doins};
    
    match name {
        "einfo" => {
            for arg in args {
                einfo(arg);
            }
            Ok(())
        }
        "ewarn" => {
            for arg in args {
                ewarn(arg);
            }
            Ok(())
        }
        "eerror" => {
            for arg in args {
                eerror(arg);
            }
            Ok(())
        }
        "die" => {
            let msg = args.join(" ");
            Err(InvalidData::new(&msg, None))
        }
        "ebegin" => {
            let msg = args.join(" ");
            ebegin(&msg);
            Ok(())
        }
        "eend" => {
            let code = args.get(0).and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
            eend(code, None);
            Ok(())
        }
        "dobin" => {
            let file_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            dobin(env, &file_refs)?;
            Ok(())
        }
        "doins" => {
            let file_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            doins(env, &file_refs)?;
            Ok(())
        }
        "default" => {
            default_src_prepare(env)?;
            Ok(())
        }
        "default_src_prepare" => {
            default_src_prepare(env)?;
            Ok(())
        }
        "default_src_unpack" => {
            default_src_unpack(env)?;
            Ok(())
        }
        _ => {
            // Unknown helper - would need bash fallback
            Err(InvalidData::new(&format!("Unknown helper function: {}", name), None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_helper_call() {
        let cmd = parse_helper_call("einfo \"Hello world\"");
        assert!(matches!(cmd, Some(BashCommand::HelperCall { .. })));
        
        if let Some(BashCommand::HelperCall { name, args }) = cmd {
            assert_eq!(name, "einfo");
            assert_eq!(args, vec!["Hello world"]);
        }
    }
    
    #[test]
    fn test_parse_assignment() {
        let cmd = parse_assignment("MY_VAR=\"value\"");
        assert!(matches!(cmd, Some(BashCommand::Assignment { .. })));
        
        if let Some(BashCommand::Assignment { name, value }) = cmd {
            assert_eq!(name, "MY_VAR");
            assert_eq!(value, "value");
        }
    }
    
    #[test]
    fn test_parse_args() {
        let args = parse_args("\"hello world\" test \"foo bar\"");
        assert_eq!(args, vec!["hello world", "test", "foo bar"]);
    }
}
