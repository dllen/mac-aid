use anyhow::Result;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct CommandDoc {
    pub package_name: String,
    pub command_name: String,
    pub man_content: String,
}

/// Extract man page content for a given command
pub fn get_man_page(command: &str) -> Result<String> {
    let output = Command::new("man")
        .arg(command)
        .env("MANWIDTH", "80") // Set consistent width for parsing
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to get man page for: {}", command);
    }

    let content = String::from_utf8(output.stdout)?;
    Ok(content)
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
                
                println!("Indexed: {}", package);
            }
            Err(_) => {
                // Some packages don't have man pages, skip them
                eprintln!("No man page for: {}", package);
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

/// Extract summary from man page (usually the NAME section)
pub fn extract_summary(man_content: &str) -> String {
    let lines: Vec<&str> = man_content.lines().collect();
    let mut in_name_section = false;
    let mut summary_lines = Vec::new();
    
    for line in lines {
        if line.contains("NAME") {
            in_name_section = true;
            continue;
        }
        
        if in_name_section {
            if line.starts_with(char::is_uppercase) && !line.starts_with(' ') {
                // New section started
                break;
            }
            
            if !line.trim().is_empty() {
                summary_lines.push(line.trim());
            }
        }
    }
    
    summary_lines.join(" ")
}
