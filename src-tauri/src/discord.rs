//! Achar, fechar e abrir o Discord.

use anyhow::{bail, Result};
use std::path::PathBuf;
use std::process::Command;

/// (executável, pasta em %LOCALAPPDATA%)
pub const VARIANTES: &[(&str, &str)] = &[
    ("Discord.exe", "Discord"),
    ("DiscordPTB.exe", "DiscordPTB"),
    ("DiscordCanary.exe", "DiscordCanary"),
];

pub fn e_executavel_do_discord(nome: &str) -> bool {
    VARIANTES.iter().any(|(exe, _)| exe.eq_ignore_ascii_case(nome))
}

#[cfg(windows)]
fn oculto(programa: impl AsRef<std::ffi::OsStr>) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut c = Command::new(programa);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

#[cfg(not(windows))]
fn oculto(programa: impl AsRef<std::ffi::OsStr>) -> Command {
    Command::new(programa)
}

fn atualizador(exe: &str) -> Option<PathBuf> {
    let pasta = VARIANTES.iter().find(|(e, _)| e.eq_ignore_ascii_case(exe))?.1;
    let base = std::env::var_os("LOCALAPPDATA")?;
    let p = PathBuf::from(base).join(pasta).join("Update.exe");
    p.exists().then_some(p)
}

/// O executável a abrir: o que estava rodando, ou o primeiro instalado.
pub fn escolher(ultimo_visto: Option<&str>) -> Option<String> {
    if let Some(u) = ultimo_visto {
        if atualizador(u).is_some() {
            return Some(u.to_string());
        }
    }
    VARIANTES
        .iter()
        .find(|(exe, _)| atualizador(exe).is_some())
        .map(|(exe, _)| exe.to_string())
}

pub fn instalado() -> bool {
    escolher(None).is_some()
}

pub fn fechar_todos() {
    for (exe, _) in VARIANTES {
        let _ = oculto("taskkill")
            .args(["/IM", exe, "/F", "/T"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

pub fn abrir(exe: &str) -> Result<()> {
    let Some(up) = atualizador(exe) else {
        bail!("não encontrei o Discord instalado ({exe})");
    };
    oculto(up).args(["--processStart", exe]).spawn()?;
    Ok(())
}
