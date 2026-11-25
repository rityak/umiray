//! Kill switch через Packet Filter: отдельный anchor не заменяет чужие правила.
//! PF не умеет разрешать процесс по пути, поэтому исходящим root-процессам,
//! включая ядро и Tailscale, дано отдельное разрешение.

use std::process::Command;

use super::{Allowed, Profile};
use crate::error::{AppError, Result};

const ANCHOR: &str = "com.umiray";

pub fn profiles() -> Result<Vec<Profile>> {
    let info = admin(&["/sbin/pfctl", "-s", "info"])?;
    let enabled = info
        .lines()
        .any(|line| line.trim_start().starts_with("Status: Enabled"));
    Ok(vec![Profile {
        name: "pf".into(),
        enabled: enabled.to_string(),
        action: String::new(),
    }])
}

pub fn apply(allowed: &Allowed) -> Result<()> {
    let device = actual_device(allowed);
    let anchor = crate::paths::Paths::run_dir().join("killswitch.pf");
    std::fs::write(&anchor, ruleset(&device))?;
    admin(&["/sbin/pfctl", "-a", ANCHOR, "-f", &anchor.to_string_lossy()])?;
    let main = crate::paths::Paths::run_dir().join("killswitch-main.pf.conf");
    let original = std::fs::read_to_string("/etc/pf.conf").unwrap_or_default();
    std::fs::write(&main, main_config(&original))?;
    admin(&["/sbin/pfctl", "-f", &main.to_string_lossy()])?;
    admin(&["/sbin/pfctl", "-E"])?;
    Ok(())
}

pub fn release(previous: &super::Backup) -> Result<()> {
    admin(&["/sbin/pfctl", "-a", ANCHOR, "-F", "all"])?;
    admin(&["/sbin/pfctl", "-f", "/etc/pf.conf"])?;
    if previous
        .profiles
        .iter()
        .any(|profile| profile.name == "pf" && profile.enabled == "false")
    {
        admin(&["/sbin/pfctl", "-d"])?;
    }
    let _ = std::fs::remove_file(crate::paths::Paths::run_dir().join("killswitch.pf"));
    let _ = std::fs::remove_file(crate::paths::Paths::run_dir().join("killswitch-main.pf.conf"));
    Ok(())
}

fn actual_device(allowed: &Allowed) -> String {
    crate::system::net::NetInfo::default_routes()
        .ok()
        .and_then(|routes| {
            routes.into_iter().find_map(|route| {
                (route.adapter.starts_with("utun") && route.gateway == "198.18.0.1")
                    .then_some(route.adapter)
            })
        })
        .unwrap_or_else(|| allowed.device.clone())
}

fn ruleset(device: &str) -> String {
    format!(
        "block out quick all\n\
         pass out quick on lo0 all\n\
         pass out quick on {device} all\n\
         pass out quick from any to 198.18.0.0/15\n\
         pass out quick from any to 10.0.0.0/8\n\
         pass out quick from any to 172.16.0.0/12\n\
         pass out quick from any to 192.168.0.0/16\n\
         pass out quick from any to 100.64.0.0/10\n\
         pass out quick from any to 224.0.0.0/4\n\
         pass out quick from any to 255.255.255.255\n\
         pass out quick from any to fe80::/10\n\
         pass out quick from any to fc00::/7\n\
         pass out quick from any to ff00::/8\n\
         pass out quick from any to any user root\n"
    )
}

fn main_config(original: &str) -> String {
    let invocation = format!("anchor \"{ANCHOR}\"");
    if original.lines().any(|line| line.trim() == invocation) {
        return original.to_string();
    }
    format!("{}\n{invocation}\n", original.trim_end())
}

fn admin(args: &[&str]) -> Result<String> {
    let out = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(
            r#"on run argv
set commandLine to ""
repeat with argumentValue in argv
    set commandLine to commandLine & quoted form of (argumentValue as text) & " "
end repeat
do shell script commandLine with administrator privileges
end run"#,
        )
        .arg("--")
        .args(args)
        .output()
        .map_err(|e| AppError::io(format!("pfctl не запустился: {e}")))?;
    if !out.status.success() {
        return Err(AppError::io(format!(
            "pfctl: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_anchor_blocks_egress_but_keeps_the_core_and_local_routes() {
        let rules = ruleset("utun7");
        assert!(rules.starts_with("block out quick all\n"));
        for expected in [
            "on lo0",
            "on utun7",
            "198.18.0.0/15",
            "192.168.0.0/16",
            "100.64.0.0/10",
            "user root",
        ] {
            assert!(rules.contains(expected), "нет {expected}:\n{rules}");
        }
    }

    #[test]
    fn the_main_config_keeps_existing_rules_and_adds_the_anchor_once() {
        let original = "set skip on lo0\nanchor \"com.apple\"\n";
        let once = main_config(original);
        assert!(once.starts_with(original));
        assert_eq!(once.matches("anchor \"com.umiray\"").count(), 1);
        assert_eq!(main_config(&once), once);
    }
}
