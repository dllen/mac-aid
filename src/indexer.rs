use anyhow::Result;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct CommandDoc {
    pub package_name: String,
    pub command_name: String,
    pub man_content: String,
}

/// Extract man page content for a given command
/// Falls back to command help options if man page is not available
pub fn get_man_page(command: &str) -> Result<String> {
    // Try man page first
    let output = Command::new("man")
        .arg(command)
        .env("MANWIDTH", "80") // Set consistent width for parsing
        .output()?;

    if output.status.success() {
        let content = String::from_utf8(output.stdout)?;
        return Ok(content);
    }

    // Fallback: try help options in order: -h, --help, -help
    let help_options = vec!["-h", "--help", "-help"];
    for option in help_options {
        if let Ok(output) = Command::new(command)
            .arg(option)
            .output()
        {
            if output.status.success() {
                if let Ok(content) = String::from_utf8(output.stdout) {
                    if !content.trim().is_empty() {
                        return Ok(content);
                    }
                }
            }
        }
    }

    // If all attempts fail, return error
    anyhow::bail!("Failed to get man page or help for: {}", command);
}

/// Index all brew packages and their man pages
pub async fn index_brew_packages(packages: &[String]) -> Result<Vec<CommandDoc>> {
    let mut docs = Vec::new();
    
    for package in packages {
        // Try to get man page for the package
        match get_man_page(package) {
            Ok(content) => {
                // Clean up the content (remove ANSI codes, etc.)
                let cleaned = clean_man_content(&content);
                
                docs.push(CommandDoc {
                    package_name: package.clone(),
                    command_name: package.clone(),
                    man_content: cleaned,
                });
                
                crate::log::log_info(&format!("Indexed: {}", package));
            }
            Err(_) => {
                // Some packages don't have man pages, skip them
                crate::log::log_info(&format!("Indexed: {}", package));
            }
        }
    }
    
    Ok(docs)
}

/// Clean man page content by removing ANSI escape codes and extra whitespace
fn clean_man_content(content: &str) -> String {
    // Remove ANSI escape codes
    let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    let cleaned = re.replace_all(content, "");
    
    // Remove backspace characters used for bold/underline
    let re = regex::Regex::new(r".\x08").unwrap();
    let cleaned = re.replace_all(&cleaned, "");
    
    // Normalize whitespace
    let lines: Vec<&str> = cleaned
        .lines()
        .map(|l| l.trim_end())
        .filter(|l| !l.is_empty())
        .collect();
    
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_man_content_removes_ansi_and_empty_lines() {
        let s = "\x1b[31mHello\x1b[0m\n\nWorld  \n";
        let out = clean_man_content(s);
        assert_eq!(out, "Hello\nWorld");
    }

    #[test]
    fn test_clean_man_content_removes_backspaces() {
        let s = "B\x08Bold\nUnder\x08lined";
        let out = clean_man_content(s);
        assert!(!out.contains('\x08'));
    }
}
