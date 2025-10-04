// ebuild_exec.rs - Ebuild function execution engine

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::fs;
use std::path::{Path, PathBuf};
use regex::Regex;
use lazy_static::lazy_static;
use crate::exception::InvalidData;
use crate::doebuild::BuildEnv;

lazy_static! {
    static ref FUNCTION_RE: Regex = Regex::new(r"^(?P<name>src_(?:unpack|prepare|configure|compile|test|install))\s*\(\)\s*\{(?P<body>.*?)\}$").unwrap();
    static ref HELPER_RE: Regex = Regex::new(r"\b(?P<helper>do(?:bin|ins|man|doc|lib|etc|initd|confd))\s+(?P<args>.*?)(?=\s|$|;)").unwrap();
}

/// Represents a parsed ebuild function
#[derive(Debug, Clone)]
pub struct EbuildFunction {
    pub name: String,
    pub body: String,
}

/// Ebuild execution engine
pub struct EbuildExecutor {
    functions: HashMap<String, EbuildFunction>,
}

impl EbuildExecutor {
    /// Create a new executor by parsing an ebuild file
    pub fn from_ebuild(ebuild_path: &Path) -> Result<Self, InvalidData> {
        let content = fs::read_to_string(ebuild_path)
            .map_err(|e| InvalidData::new(&format!("Failed to read ebuild: {}", e), None))?;

        let functions = Self::parse_functions(&content)?;
        Ok(EbuildExecutor { functions })
    }

    /// Parse functions from ebuild content
    fn parse_functions(content: &str) -> Result<HashMap<String, EbuildFunction>, InvalidData> {
        let mut functions = HashMap::new();

        // Simple function parsing - look for src_* functions
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Look for function start
            if line.starts_with("src_") && line.contains("() {") {
                let func_name = line.split("()").next().unwrap().trim();

                // Find the matching closing brace
                let mut brace_count = 0;
                let mut func_body = String::new();
                let mut found_start = false;

                for j in i..lines.len() {
                    let current_line = lines[j];

                    for ch in current_line.chars() {
                        if ch == '{' {
                            brace_count += 1;
                            found_start = true;
                        } else if ch == '}' {
                            brace_count -= 1;
                        }
                    }

                    if found_start {
                        func_body.push_str(current_line);
                        func_body.push('\n');
                    }

                    if found_start && brace_count == 0 {
                        // Remove the function declaration line and closing brace
                        let body_lines: Vec<&str> = func_body.lines().collect();
                        let body_content = body_lines[1..body_lines.len()-1].join("\n");

                        functions.insert(func_name.to_string(), EbuildFunction {
                            name: func_name.to_string(),
                            body: body_content,
                        });
                        i = j;
                        break;
                    }
                }
            }
            i += 1;
        }

        Ok(functions)
    }

    /// Check if a specific function exists
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    /// Execute a specific ebuild function
    pub fn execute_function(&self, name: &str, build_env: &BuildEnv) -> Result<(), InvalidData> {
        let function = self.functions.get(name)
            .ok_or_else(|| InvalidData::new(&format!("Function {} not found", name), None))?;

        // Create a bash script with the function
        let script = self.create_bash_script(&function.body, build_env)?;

        // Execute the script
        let output = Command::new("bash")
            .arg("-c")
            .arg(&script)
            .current_dir(&build_env.workdir)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|e| InvalidData::new(&format!("Failed to execute {}: {}", name, e), None))?;

        if !output.status.success() {
            return Err(InvalidData::new(&format!("Function {} failed", name), None));
        }

        Ok(())
    }

    /// Create a bash script with proper environment setup
    fn create_bash_script(&self, body: &str, build_env: &BuildEnv) -> Result<String, InvalidData> {
        let mut script = String::new();

        // Set up environment variables
        script.push_str("#!/bin/bash\n");
        script.push_str("set -e\n\n");

        // Export build environment variables
        for (key, value) in &build_env.env_vars {
            script.push_str(&format!("export {}=\"{}\"\n", key, value));
        }

        // Add helper functions
        script.push_str("\n# Ebuild helper functions\n");
        script.push_str(&self.generate_helper_functions());

        // Add the function body
        script.push_str("\n# Function body\n");
        script.push_str(body);
        script.push_str("\n");

        Ok(script)
    }

    /// Generate basic ebuild helper functions
    fn generate_helper_functions(&self) -> String {
        let mut helpers = String::new();

        // dobin - install binary
        helpers.push_str("dobin() {\n");
        helpers.push_str("    for file in \"$@\"; do\n");
        helpers.push_str("        if [ -f \"$file\" ]; then\n");
        helpers.push_str("            install -D -m0755 \"$file\" \"$D/usr/bin/$(basename \"$file\")\"\n");
        helpers.push_str("        else\n");
        helpers.push_str("            echo \"dobin: $file not found\" >&2\n");
        helpers.push_str("            return 1\n");
        helpers.push_str("        fi\n");
        helpers.push_str("    done\n");
        helpers.push_str("}\n\n");

        // doins - install file
        helpers.push_str("doins() {\n");
        helpers.push_str("    for file in \"$@\"; do\n");
        helpers.push_str("        if [ -f \"$file\" ]; then\n");
        helpers.push_str("            install -D -m0644 \"$file\" \"$D/usr/share/$(basename \"$file\")\"\n");
        helpers.push_str("        else\n");
        helpers.push_str("            echo \"doins: $file not found\" >&2\n");
        helpers.push_str("            return 1\n");
        helpers.push_str("        fi\n");
        helpers.push_str("    done\n");
        helpers.push_str("}\n\n");

        // doman - install man page
        helpers.push_str("doman() {\n");
        helpers.push_str("    for file in \"$@\"; do\n");
        helpers.push_str("        if [ -f \"$file\" ]; then\n");
        helpers.push_str("            # Extract section from filename (e.g., man1, man8)\n");
        helpers.push_str("            section=\"${file##*.}\"\n");
        helpers.push_str("            install -D -m0644 \"$file\" \"$D/usr/share/man/man$section/$(basename \"$file\")\"\n");
        helpers.push_str("        else\n");
        helpers.push_str("            echo \"doman: $file not found\" >&2\n");
        helpers.push_str("            return 1\n");
        helpers.push_str("        fi\n");
        helpers.push_str("    done\n");
        helpers.push_str("}\n\n");

        // dodoc - install documentation
        helpers.push_str("dodoc() {\n");
        helpers.push_str("    for file in \"$@\"; do\n");
        helpers.push_str("        if [ -f \"$file\" ]; then\n");
        helpers.push_str("            install -D -m0644 \"$file\" \"$D/usr/share/doc/${PF}/$(basename \"$file\")\"\n");
        helpers.push_str("        else\n");
        helpers.push_str("            echo \"dodoc: $file not found\" >&2\n");
        helpers.push_str("            return 1\n");
        helpers.push_str("        fi\n");
        helpers.push_str("    done\n");
        helpers.push_str("}\n\n");

        // default - run default implementation
        helpers.push_str("default() {\n");
        helpers.push_str("    # Default implementation - currently a no-op\n");
        helpers.push_str("    true\n");
        helpers.push_str("}\n\n");

        // emake - run make with proper flags
        helpers.push_str("emake() {\n");
        helpers.push_str("    make \"$@\"\n");
        helpers.push_str("}\n\n");

        helpers
    }
}