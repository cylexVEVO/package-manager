use crate::PackageSource;
use anyhow::{bail, Context, Result};
use std::{
    collections::HashMap,
    process::{Command, Stdio},
};

// Missing tools and failed queries are unknown, never evidence that an app is absent.
#[derive(Default)]
pub struct InstalledPackages {
    flatpak: Option<Vec<String>>,
    pacman: Option<Vec<String>>,
    nix: Option<HashMap<String, Vec<String>>>,
}

fn output(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("starting {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn nix_elements(json: &str) -> Result<HashMap<String, Vec<String>>> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let elements = value["elements"]
        .as_object()
        .context("unsupported Nix profile format")?;
    let mut result: HashMap<String, Vec<String>> = HashMap::new();
    for (name, element) in elements {
        if element["originalUrl"].as_str() != Some("flake:nixpkgs")
            && element["originalUrl"].as_str() != Some("nixpkgs")
        {
            continue;
        }
        if let Some(attr) = element["attrPath"].as_str() {
            // legacyPackages.<system>.<attribute>, preserving dotted attributes.
            if let Some(package) = attr.splitn(3, '.').nth(2) {
                result
                    .entry(package.to_owned())
                    .or_default()
                    .push(name.clone());
            }
        }
    }
    Ok(result)
}

impl InstalledPackages {
    pub fn query() -> Self {
        Self {
            flatpak: output(
                "flatpak",
                &["list", "--system", "--app", "--columns=application,branch"],
            )
            .ok()
            .map(|s| {
                s.lines()
                    .filter_map(|line| {
                        let mut fields = line.split_whitespace();
                        let id = fields.next()?;
                        (fields.next()? == "stable").then(|| id.to_owned())
                    })
                    .collect()
            }),
            pacman: output("pacman", &["-Qq"])
                .ok()
                .map(|s| s.lines().map(str::to_owned).collect()),
            nix: output(
                "nix",
                &[
                    "--extra-experimental-features",
                    "nix-command flakes",
                    "profile",
                    "list",
                    "--json",
                ],
            )
            .and_then(|s| nix_elements(&s))
            .ok(),
        }
    }

    pub fn contains(&self, source: &PackageSource) -> Option<bool> {
        match source.manager.as_str() {
            "flatpak" => self.flatpak.as_ref().map(|v| v.contains(&source.package)),
            "pacman" | "aur" => self.pacman.as_ref().map(|v| v.contains(&source.package)),
            "nix" => self.nix.as_ref().map(|v| v.contains_key(&source.package)),
            _ => None,
        }
    }
}

pub(crate) fn uninstall(source: &PackageSource) -> Result<()> {
    let mut command;
    let _lock;
    match source.manager.as_str() {
        "flatpak" => {
            let arch = libflatpak::default_arch().context("determining Flatpak architecture")?;
            command = Command::new("flatpak");
            command
                .args(["uninstall", "--system", "--assumeyes", "--"])
                .arg(format!("app/{}/{arch}/stable", source.package));
        }
        "pacman" | "aur" => {
            _lock = crate::paru::lock(std::path::Path::new("/var/lib/pacman"))?;
            command = Command::new("paru");
            // Plain -R preserves dependencies and refuses to break other packages.
            command
                .args([
                    "--sudo",
                    "pkexec",
                    "--nosudoloop",
                    "-R",
                    "--noconfirm",
                    "--",
                ])
                .arg(&source.package);
        }
        "nix" => {
            let elements = nix_elements(&output(
                "nix",
                &[
                    "--extra-experimental-features",
                    "nix-command flakes",
                    "profile",
                    "list",
                    "--json",
                ],
            )?)?;
            let names = elements
                .get(&source.package)
                .context("app is not in the Nix profile")?;
            command = Command::new("nix");
            command
                .args([
                    "--extra-experimental-features",
                    "nix-command flakes",
                    "profile",
                    "remove",
                    "--",
                ])
                .args(names);
        }
        manager => bail!("package manager {manager:?} is not supported"),
    }
    let output = command
        .stdin(Stdio::null())
        .output()
        .context("starting uninstall")?;
    if !output.status.success() {
        bail!(
            "uninstall failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nix_uses_exact_attributes_and_preserves_profile_names() {
        let entries = nix_elements(r#"{"elements":{"renamed":{"originalUrl":"flake:nixpkgs","attrPath":"legacyPackages.x86_64-linux.foo.bar"},"other":{"originalUrl":"flake:other","attrPath":"legacyPackages.x86_64-linux.foo.bar"}}}"#).unwrap();
        assert_eq!(entries["foo.bar"], vec!["renamed"]);
        assert!(!entries.contains_key("bar"));
        assert!(nix_elements("{}").is_err());
    }
    #[test]
    fn unavailable_backend_is_unknown() {
        let installed = InstalledPackages::default();
        assert_eq!(
            installed.contains(&PackageSource {
                manager: "flatpak".into(),
                package: "org.example.App".into()
            }),
            None
        );
    }
}
