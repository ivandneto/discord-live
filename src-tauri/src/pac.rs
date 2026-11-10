//! O arquivo PAC e a única marca que o programa deixa no sistema: o "proxy
//! automático" do Windows (`HKCU\...\Internet Settings\AutoConfigURL`).
//!
//! O Discord relê o PAC em toda abertura, então a correção vale sempre, sem
//! mexer em atalho nem em arquivo do Discord. É HKCU: não precisa de
//! administrador, e desligar é devolver o valor anterior. Ideia do FOL-discord.
//!
//! Se o programa morrer com o PAC ligado, o Discord não acha o arquivo e cai
//! para a conexão direta: perde-se a correção, nunca a internet.

use crate::rotas;
use anyhow::{bail, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub const CAMINHO: &str = "/live-ponte.pac";

pub fn url(porta: u16) -> String {
    format!("http://127.0.0.1:{porta}{CAMINHO}")
}

pub fn e_nosso(valor: &str) -> bool {
    valor.starts_with("http://127.0.0.1:") && valor.ends_with(CAMINHO)
}

/// Sobe o servidor do PAC numa porta livre a partir de `preferida`.
pub async fn servir(preferida: u16, porta_socks: u16) -> Result<u16> {
    for p in preferida..preferida + 20 {
        if let Ok(escuta) = TcpListener::bind(("127.0.0.1", p)).await {
            let corpo = rotas_pac(porta_socks);
            tokio::spawn(async move {
                loop {
                    if let Ok((c, _)) = escuta.accept().await {
                        let corpo = corpo.clone();
                        tokio::spawn(async move {
                            let _ = responder(c, corpo).await;
                        });
                    }
                }
            });
            return Ok(p);
        }
    }
    bail!("nenhuma porta livre para o PAC")
}

fn rotas_pac(porta_socks: u16) -> String {
    let _ = rotas::decide_regiao; // o PAC entrega o domínio inteiro; quem decide é rotas.rs
    format!(
        r#"function FindProxyForURL(url, host) {{
  if (dnsDomainIs(host, ".discord.com")    || host == "discord.com"    ||
      dnsDomainIs(host, ".discord.gg")     || host == "discord.gg"     ||
      dnsDomainIs(host, ".discord.media")  ||
      dnsDomainIs(host, ".discordapp.com") || host == "discordapp.com")
    return "SOCKS5 127.0.0.1:{porta_socks}";
  return "DIRECT";
}}
"#
    )
}

async fn responder(mut c: TcpStream, corpo: String) -> Result<()> {
    let mut lixo = [0u8; 2048];
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), c.read(&mut lixo)).await;
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{}",
        corpo.len(),
        corpo
    );
    c.write_all(resp.as_bytes()).await?;
    c.shutdown().await?;
    Ok(())
}

// --- Registro do Windows ----------------------------------------------------

#[cfg(windows)]
mod sistema {
    use super::e_nosso;
    use anyhow::{Context, Result};
    use winreg::{enums::*, RegKey};

    const CHAVE: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    const BACKUP: &str = "AutoConfigURL_backup_LivePonte";

    fn avisar_windows() {
        use windows_sys::Win32::Networking::WinInet::{
            InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
        };
        unsafe {
            InternetSetOptionW(std::ptr::null(), INTERNET_OPTION_SETTINGS_CHANGED, std::ptr::null(), 0);
            InternetSetOptionW(std::ptr::null(), INTERNET_OPTION_REFRESH, std::ptr::null(), 0);
        }
    }

    /// Aponta o proxy automático para o nosso PAC. Devolve o valor estranho que
    /// estava lá antes (se havia), que fica guardado para ser devolvido.
    pub fn ativar(url: &str) -> Result<Option<String>> {
        let (k, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(CHAVE)
            .context("abrindo Internet Settings")?;
        let atual: Option<String> = k
            .get_value::<String, _>("AutoConfigURL")
            .ok()
            .filter(|v| !v.is_empty());
        let mut estranho = None;
        match &atual {
            // Valor de outra pessoa: o atual sempre vence um backup antigo.
            Some(a) if !e_nosso(a) => {
                k.set_value(BACKUP, a)?;
                estranho = Some(a.clone());
            }
            // Já é nosso (o programa caiu e voltou): o backup existente continua valendo.
            Some(_) => {}
            // Não havia nada: não pode sobrar backup velho para ressuscitar depois.
            None => {
                let _ = k.delete_value(BACKUP);
            }
        }
        if atual.as_deref() != Some(url) {
            k.set_value("AutoConfigURL", &url.to_string())?;
            avisar_windows();
        }
        Ok(estranho)
    }

    pub fn desativar() -> Result<()> {
        let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(CHAVE)?;
        let atual: Option<String> = k.get_value("AutoConfigURL").ok();
        if !atual.as_deref().is_some_and(e_nosso) {
            return Ok(()); // não é nosso: não mexe
        }
        match k.get_value::<String, _>(BACKUP) {
            Ok(anterior) => {
                k.set_value("AutoConfigURL", &anterior)?;
                let _ = k.delete_value(BACKUP);
            }
            Err(_) => {
                let _ = k.delete_value("AutoConfigURL");
            }
        }
        avisar_windows();
        Ok(())
    }

    pub fn ativo() -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(CHAVE)
            .and_then(|k| k.get_value::<String, _>("AutoConfigURL"))
            .map(|v| e_nosso(&v))
            .unwrap_or(false)
    }
}

#[cfg(not(windows))]
mod sistema {
    use anyhow::Result;
    // Mac e Linux entram na próxima etapa.
    pub fn ativar(_url: &str) -> Result<Option<String>> {
        Ok(None)
    }
    pub fn desativar() -> Result<()> {
        Ok(())
    }
    pub fn ativo() -> bool {
        false
    }
}

pub use sistema::{ativar, ativo, desativar};

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn pac_manda_so_o_discord_ao_proxy() {
        let p = rotas_pac(9350);
        assert!(p.contains("SOCKS5 127.0.0.1:9350"));
        assert!(p.contains("return \"DIRECT\""));
        assert!(p.contains(".discord.media"));
    }

    #[test]
    fn reconhece_a_propria_url() {
        assert!(e_nosso(&url(9351)));
        assert!(!e_nosso("http://127.0.0.1:9251/proxy.pac"));
        assert!(!e_nosso("http://wpad.empresa/proxy.pac"));
    }
}
