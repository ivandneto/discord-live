//! Testa uma saída: está de pé? de que país sai? e, principalmente, que região
//! o Discord entrega para quem vem por ela?
//!
//! `latency.discord.media/rtc` devolve a lista de regiões conforme o IP de quem
//! pergunta. Se a primeira for `brazil`, a saída não muda nada.

use crate::conector;
use crate::saidas::{ResultadoTeste, Saida};
use anyhow::{anyhow, bail, Result};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::rustls::{self, pki_types::ServerName, ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

const PRAZO_HTTP: Duration = Duration::from_secs(12);

fn tls() -> TlsConnector {
    static CFG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    let cfg = CFG.get_or_init(|| {
        let mut raizes = RootCertStore::empty();
        raizes.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let provedor = Arc::new(rustls::crypto::ring::default_provider());
        let cfg = ClientConfig::builder_with_provider(provedor)
            .with_safe_default_protocol_versions()
            .expect("versões TLS padrão")
            .with_root_certificates(raizes)
            .with_no_client_auth();
        Arc::new(cfg)
    });
    TlsConnector::from(cfg.clone())
}

async fn abrir(saida: Option<&Saida>, host: &str) -> Result<TcpStream> {
    match saida {
        Some(s) => conector::via(s, host, 443, conector::PRAZO_SAIDA).await,
        None => conector::direto(host, 443).await,
    }
}

/// GET simples em HTTPS, devolvendo o corpo como texto.
pub async fn get(saida: Option<&Saida>, host: &str, caminho: &str) -> Result<String> {
    let tcp = abrir(saida, host).await?;
    let nome = ServerName::try_from(host.to_string()).map_err(|_| anyhow!("nome inválido"))?;
    let fut = async {
        let mut t = tls().connect(nome, tcp).await?;
        let req = format!(
            "GET {caminho} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: LivePonte/{}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
            env!("CARGO_PKG_VERSION")
        );
        t.write_all(req.as_bytes()).await?;
        let mut buf = Vec::new();
        // Alguns servidores encerram o TLS sem close_notify; o que já chegou basta.
        let _ = t.read_to_end(&mut buf).await;
        Ok::<_, anyhow::Error>(buf)
    };
    let buf = timeout(PRAZO_HTTP, fut)
        .await
        .map_err(|_| anyhow!("{host} não respondeu a tempo"))??;
    let texto = String::from_utf8_lossy(&buf).into_owned();
    let (cab, corpo) = texto.split_once("\r\n\r\n").ok_or_else(|| anyhow!("resposta vazia de {host}"))?;
    let status = cab.lines().next().unwrap_or("");
    if !status.contains(" 200") {
        bail!("{host} respondeu {status}");
    }
    Ok(corpo.to_string())
}

/// Lê o primeiro `"region": "..."` da resposta de latency.discord.media/rtc.
pub fn primeira_regiao(corpo: &str) -> Option<String> {
    let i = corpo.find("\"region\"")?;
    let resto = &corpo[i + 8..];
    let resto = resto.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let fim = resto.find('"')?;
    Some(resto[..fim].to_string())
}

fn campo_trace<'a>(corpo: &'a str, chave: &str) -> Option<&'a str> {
    corpo
        .lines()
        .find_map(|l| l.strip_prefix(chave).and_then(|r| r.strip_prefix('=')))
        .map(str::trim)
}

fn agora_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `None` testa a conexão direta desta máquina.
pub async fn testar(saida: Option<&Saida>) -> ResultadoTeste {
    let mut r = ResultadoTeste {
        quando: agora_ms(),
        ..Default::default()
    };

    // Latência: tempo de abrir um túnel até a borda do Discord.
    let t0 = Instant::now();
    match abrir(saida, "discord.com").await {
        Ok(_) => r.ping_ms = Some(t0.elapsed().as_millis() as u32),
        Err(e) => {
            r.erro = Some(e.to_string());
            return r;
        }
    }

    // País e IP de saída (Cloudflare trace).
    if let Ok(corpo) = get(saida, "one.one.one.one", "/cdn-cgi/trace").await {
        r.ip = campo_trace(&corpo, "ip").map(str::to_string);
        r.pais = campo_trace(&corpo, "loc").map(str::to_string);
    }

    // A pergunta que importa: que região o Discord escolhe para este IP?
    match get(saida, "latency.discord.media", "/rtc").await {
        Ok(corpo) => match primeira_regiao(&corpo) {
            Some(reg) => {
                r.ok = reg != "brazil";
                if !r.ok {
                    r.erro = Some("o Discord continua vendo esta saída como Brasil".into());
                }
                r.regiao = Some(reg);
            }
            None => r.erro = Some("o Discord não informou a região".into()),
        },
        Err(e) => r.erro = Some(e.to_string()),
    }
    r
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_a_primeira_regiao() {
        let j = r#"[{"region": "brazil", "ips": ["1.2.3.4"]}, {"region":"santiago"}]"#;
        assert_eq!(primeira_regiao(j).as_deref(), Some("brazil"));
        assert_eq!(primeira_regiao(r#"[{"region":"us-east"}]"#).as_deref(), Some("us-east"));
        assert_eq!(primeira_regiao("[]"), None);
    }

    #[test]
    fn le_o_trace() {
        let t = "fl=1\nip=200.1.2.3\nloc=CL\n";
        assert_eq!(campo_trace(t, "ip"), Some("200.1.2.3"));
        assert_eq!(campo_trace(t, "loc"), Some("CL"));
        assert_eq!(campo_trace(t, "x"), None);
    }
}
