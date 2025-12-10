use anyhow::Result;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct BrewPackage {
    pub name: String,
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
        })
        .collect();

    Ok(packages)
}
