//! Abre uma conexão TCP até um destino passando pela saída (SOCKS5 ou HTTP CONNECT).

use crate::saidas::{Saida, Tipo};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Prazo do aperto de mão inteiro (TCP + protocolo do proxy).
pub const PRAZO_SAIDA: Duration = Duration::from_secs(6);
pub const PRAZO_DIRETO: Duration = Duration::from_secs(10);

pub async fn direto(host: &str, porta: u16) -> Result<TcpStream> {
    let t = timeout(PRAZO_DIRETO, TcpStream::connect((host, porta)))
        .await
        .map_err(|_| anyhow!("tempo esgotado conectando direto a {host}"))??;
    let _ = t.set_nodelay(true);
    Ok(t)
}

pub async fn via(saida: &Saida, host: &str, porta: u16, prazo: Duration) -> Result<TcpStream> {
    timeout(prazo, via_sem_prazo(saida, host, porta))
        .await
        .map_err(|_| anyhow!("a saída \"{}\" não respondeu a tempo", saida.nome))?
}

async fn via_sem_prazo(saida: &Saida, host: &str, porta: u16) -> Result<TcpStream> {
    let mut t = TcpStream::connect((saida.host.as_str(), saida.porta))
        .await
        .with_context(|| format!("não consegui alcançar a saída {}:{}", saida.host, saida.porta))?;
    let _ = t.set_nodelay(true);
    match saida.tipo {
        Tipo::Socks5 => socks5(&mut t, saida, host, porta).await?,
        Tipo::Http => http_connect(&mut t, saida, host, porta).await?,
    }
    Ok(t)
}

async fn socks5(t: &mut TcpStream, s: &Saida, host: &str, porta: u16) -> Result<()> {
    let com_senha = s.usuario.is_some();
    if com_senha {
        t.write_all(&[5, 2, 0, 2]).await?;
    } else {
        t.write_all(&[5, 1, 0]).await?;
    }
    let mut r = [0u8; 2];
    t.read_exact(&mut r).await.context("a saída fechou no cumprimento SOCKS5")?;
    if r[0] != 5 {
        bail!("a saída não fala SOCKS5");
    }
    match r[1] {
        0 => {}
        2 => {
            let u = s.usuario.as_deref().unwrap_or("").as_bytes();
            let p = s.senha.as_deref().unwrap_or("").as_bytes();
            let mut m = vec![1, u.len() as u8];
            m.extend_from_slice(u);
            m.push(p.len() as u8);
            m.extend_from_slice(p);
            t.write_all(&m).await?;
            let mut a = [0u8; 2];
            t.read_exact(&mut a).await?;
            if a[1] != 0 {
                bail!("a saída recusou usuário ou senha");
            }
        }
        0xFF => bail!("a saída exige um tipo de autenticação que não temos (usuário e senha?)"),
        m => bail!("a saída pediu um método de autenticação desconhecido ({m})"),
    }

    let hb = host.as_bytes();
    if hb.len() > 255 {
        bail!("nome de host longo demais");
    }
    let mut req = vec![5, 1, 0, 3, hb.len() as u8];
    req.extend_from_slice(hb);
    req.extend_from_slice(&porta.to_be_bytes());
    t.write_all(&req).await?;

    let mut cab = [0u8; 4];
    t.read_exact(&mut cab).await?;
    if cab[1] != 0 {
        bail!("a saída não conseguiu chegar ao destino ({})", erro_socks(cab[1]));
    }
    let resto = match cab[3] {
        1 => 4 + 2,
        4 => 16 + 2,
        3 => {
            let mut l = [0u8; 1];
            t.read_exact(&mut l).await?;
            l[0] as usize + 2
        }
        _ => bail!("resposta SOCKS5 malformada"),
    };
    let mut lixo = vec![0u8; resto];
    t.read_exact(&mut lixo).await?;
    Ok(())
}

fn erro_socks(c: u8) -> &'static str {
    match c {
        1 => "falha geral",
        2 => "proibido pelas regras do proxy",
        3 => "rede inalcançável",
        4 => "host inalcançável",
        5 => "conexão recusada",
        6 => "TTL expirou",
        7 => "comando não suportado",
        8 => "tipo de endereço não suportado",
        _ => "erro desconhecido",
    }
}

async fn http_connect(t: &mut TcpStream, s: &Saida, host: &str, porta: u16) -> Result<()> {
    let mut req = format!("CONNECT {host}:{porta} HTTP/1.1\r\nHost: {host}:{porta}\r\n");
    if let Some(u) = &s.usuario {
        let cred = STANDARD.encode(format!("{}:{}", u, s.senha.as_deref().unwrap_or("")));
        req.push_str(&format!("Proxy-Authorization: Basic {cred}\r\n"));
    }
    req.push_str("\r\n");
    t.write_all(req.as_bytes()).await?;

    // Lê até o fim dos cabeçalhos, byte a byte para não engolir dados do túnel.
    let mut resp = Vec::with_capacity(256);
    let mut b = [0u8; 1];
    while !resp.ends_with(b"\r\n\r\n") {
        if resp.len() > 16 * 1024 {
            bail!("resposta do proxy HTTP grande demais");
        }
        let n = t.read(&mut b).await?;
        if n == 0 {
            bail!("o proxy HTTP fechou a conexão");
        }
        resp.push(b[0]);
    }
    let linha = String::from_utf8_lossy(&resp);
    let status = linha.lines().next().unwrap_or("");
    let codigo = status.split_whitespace().nth(1).unwrap_or("");
    match codigo {
        "200" => Ok(()),
        "407" => bail!("o proxy HTTP recusou usuário ou senha"),
        _ => bail!("o proxy HTTP respondeu: {status}"),
    }
}
