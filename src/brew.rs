use anyhow::Result;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct BrewPackage {
    pub name: String,
    #[allow(dead_code)]
    pub description: Option<String>,
}

pub fn get_installed_packages() -> Result<Vec<BrewPackage>> {
    let output = Command::new("brew")
        .arg("list")
        .arg("--formula")
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to execute brew list command");
    }

    let stdout = String::from_utf8(output.stdout)?;
    let packages: Vec<BrewPackage> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|name| BrewPackage {
            name: name.trim().to_string(),
            description: None,
        })
        .collect();

    Ok(packages)
}

#[allow(dead_code)]
pub fn get_package_info(package_name: &str) -> Result<Option<String>> {
    let output = Command::new("brew")
        .arg("info")
        .arg(package_name)
        .output()?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8(output.stdout)?;
    // Extract the first line which usually contains the description
    let description = stdout
        .lines()
        .nth(1)
        .map(|s| s.trim().to_string());

    Ok(description)
}
