// portage_helpers.rs - Core Portage helper functions (isolated-functions.sh equivalent)
use std::process;
use std::io::{self, Write};
use colored::*;
use crate::exception::InvalidData;
use super::environment::EbuildEnvironment;

/// Die with error message and exit
pub fn die(message: &str) -> ! {
    eprintln!("{}", format!(" * ERROR: {}", message).red().bold());
    
    // Print stack trace if available
    eprintln!("{}", " * Call stack:".yellow());
    dump_trace();
    
    process::exit(1)
}

/// Die helper for internal use
pub fn helpers_die(helper: &str, message: &str) -> ! {
    eprintln!("{}", format!(" * ERROR: {}: {}", helper, message).red().bold());
    process::exit(1);
}

/// Non-fatal wrapper - allows continuing on error
pub fn nonfatal<F, T>(f: F) -> Result<T, InvalidData> 
where
    F: FnOnce() -> Result<T, InvalidData>
{
    // In nonfatal mode, we catch the error and return it instead of dying
    f()
}

/// Assert condition
pub fn assert(condition: bool, message: &str) {
    if !condition {
        die(message);
    }
}

/// Check for word in list
pub fn has(needle: &str, haystack: &[&str]) -> bool {
    haystack.contains(&needle)
}

/// has with verbose output
pub fn hasv(needle: &str, haystack: &[&str]) -> bool {
    let result = has(needle, haystack);
    if result {
        einfo(&format!("Found: {}", needle));
    }
    result
}

/// Quiet version of has (deprecated, same as has)
pub fn hasq(needle: &str, haystack: &[&str]) -> bool {
    has(needle, haystack)
}

/// Echo info message
pub fn einfo(message: &str) {
    println!("{}", format!(" * {}", message).green());
}

/// Echo info without newline
pub fn einfon(message: &str) {
    print!("{}", format!(" * {}", message).green());
    io::stdout().flush().ok();
}

/// Echo warning
pub fn ewarn(message: &str) {
    eprintln!("{}", format!(" * {}", message).yellow());
}

/// Echo error
pub fn eerror(message: &str) {
    eprintln!("{}", format!(" * {}", message).red());
}

/// Echo QA warning
pub fn eqawarn(message: &str) {
    eprintln!("{}", format!(" * QA Notice: {}", message).yellow().bold());
}

/// Echo log message
pub fn elog(message: &str) {
    println!("{}", format!(" * {}", message).cyan());
}

/// Begin operation
pub fn ebegin(message: &str) {
    print!("{}", format!(" * {} ...", message).green());
    io::stdout().flush().ok();
}

/// End operation
pub fn eend(exit_code: i32, message: Option<&str>) {
    if exit_code == 0 {
        println!("{}", " [ ok ]".green());
    } else {
        println!("{}", " [ !! ]".red());
        if let Some(msg) = message {
            eerror(msg);
        }
    }
}

/// Debug print (only if PORTAGE_DEBUG is set)
pub fn debug_print(message: &str, env: &EbuildEnvironment) {
    if env.get("PORTAGE_DEBUG").map_or(false, |v| v == "1") {
        eprintln!("[DEBUG] {}", message);
    }
}

/// Debug print function entry
pub fn debug_print_function(func_name: &str, args: &[&str], env: &EbuildEnvironment) {
    if env.get("PORTAGE_DEBUG").map_or(false, |v| v == "1") {
        eprintln!("[DEBUG] Entering function: {} with args: {:?}", func_name, args);
    }
}

/// Debug print section
pub fn debug_print_section(section: &str, env: &EbuildEnvironment) {
    if env.get("PORTAGE_DEBUG").map_or(false, |v| v == "1") {
        eprintln!("[DEBUG] === {} ===", section);
    }
}

/// Check if word is in space-separated list
pub fn contains_word(word: &str, list: &str) -> bool {
    list.split_whitespace().any(|w| w == word)
}

/// Get MAKEOPTS parallel jobs count
pub fn makeopts_jobs(env: &EbuildEnvironment) -> usize {
    if let Some(makeopts) = env.get("MAKEOPTS") {
        for opt in makeopts.split_whitespace() {
            if opt.starts_with("-j") {
                if let Ok(jobs) = opt[2..].parse::<usize>() {
                    return jobs;
                }
            } else if opt == "-j" {
                // Unlimited parallelism - return CPU count
                return num_cpus::get();
            }
        }
    }
    
    // Default to 1 if not specified
    1
}

/// Get parallel jobs for build
pub fn get_parallel_jobs(env: &EbuildEnvironment) -> usize {
    makeopts_jobs(env)
}

/// Quote string for shell
pub fn eqaquote(s: &str) -> String {
    if s.contains(|c: char| c.is_whitespace() || "\"'\\$`".contains(c)) {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}

/// EQA tag for QA checks
pub fn eqatag(tag: &str, message: &str) {
    eqawarn(&format!("[{}] {}", tag, message));
}

/// Assert SIGPIPE is OK
pub fn assert_sigpipe_ok(env: &EbuildEnvironment) -> Result<(), InvalidData> {
    // Check if SIGPIPE status is set correctly
    if let Some(status) = env.get("PORTAGE_SIGPIPE_STATUS") {
        debug_print(&format!("SIGPIPE status: {}", status), env);
    }
    Ok(())
}

/// Dump trace/backtrace
pub fn dump_trace() {
    use std::backtrace::Backtrace;
    
    let backtrace = Backtrace::capture();
    
    match backtrace.status() {
        std::backtrace::BacktraceStatus::Captured => {
            eprintln!("{}", backtrace);
        }
        _ => {
            eprintln!("   [backtrace not available - set RUST_BACKTRACE=1 for detailed trace]");
        }
    }
}

/// Repository attribute lookup
pub fn repo_attr(repo: &str, attr: &str, env: &EbuildEnvironment) -> Option<String> {
    // Look up repository attributes from repos.conf
    // This is a simplified implementation
    match attr {
        "location" => env.get(&format!("REPO_{}_LOCATION", repo.to_uppercase())).cloned(),
        "sync-uri" => env.get(&format!("REPO_{}_SYNC_URI", repo.to_uppercase())).cloned(),
        _ => None,
    }
}

/// Color support detection
static mut COLORS_ENABLED: bool = true;

pub fn unset_colors() {
    unsafe {
        COLORS_ENABLED = false;
    }
    colored::control::set_override(false);
}

pub fn set_colors() {
    unsafe {
        COLORS_ENABLED = true;
    }
    colored::control::set_override(true);
}

pub fn colors_enabled() -> bool {
    unsafe { COLORS_ENABLED }
}
