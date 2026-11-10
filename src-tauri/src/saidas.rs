//! A saída estrangeira: um proxy SOCKS5 ou HTTP escolhido pelo usuário.
//! Inclui a leitura de endereços colados e o código de convite.

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tipo {
    Socks5,
    Http,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResultadoTeste {
    pub ok: bool,
    /// Unix ms.
    pub quando: u64,
    pub ping_ms: Option<u32>,
    pub ip: Option<String>,
    pub pais: Option<String>,
    pub regiao: Option<String>,
    pub erro: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Saida {
    pub id: String,
    pub nome: String,
    pub tipo: Tipo,
    pub host: String,
    pub porta: u16,
    #[serde(default)]
    pub usuario: Option<String>,
    #[serde(default)]
    pub senha: Option<String>,
    #[serde(default)]
    pub teste: Option<ResultadoTeste>,
}

/// O que a interface vê: nunca a senha.
#[derive(Clone, Debug, Serialize)]
pub struct SaidaPublica {
    pub id: String,
    pub nome: String,
    pub tipo: Tipo,
    pub host: String,
    pub porta: u16,
    pub usuario: Option<String>,
    pub tem_senha: bool,
    pub teste: Option<ResultadoTeste>,
}

impl Saida {
    pub fn publica(&self) -> SaidaPublica {
        SaidaPublica {
            id: self.id.clone(),
            nome: self.nome.clone(),
            tipo: self.tipo,
            host: self.host.clone(),
            porta: self.porta,
            usuario: self.usuario.clone(),
            tem_senha: self.senha.as_deref().is_some_and(|s| !s.is_empty()),
            teste: self.teste.clone(),
        }
    }

    pub fn mesmo_destino(&self, outra: &Saida) -> bool {
        self.tipo == outra.tipo
            && self.host.eq_ignore_ascii_case(&outra.host)
            && self.porta == outra.porta
            && self.usuario == outra.usuario
    }
}

pub fn novo_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("s{:x}", n)
}

fn decodificar_pct(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `:` só é aceito em IPv6 (que chega aqui já sem os colchetes).
fn host_valido(h: &str, ipv6: bool) -> bool {
    if ipv6 {
        return h.parse::<std::net::Ipv6Addr>().is_ok();
    }
    !h.is_empty()
        && h.len() <= 253
        && h.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// Separa `host:porta`, aceitando IPv6 entre colchetes.
fn host_porta(s: &str) -> Result<(String, u16)> {
    let ipv6 = s.starts_with('[');
    let (h, p) = if let Some(resto) = s.strip_prefix('[') {
        let fim = resto.find(']').ok_or_else(|| anyhow!("IPv6 sem ']'"))?;
        let h = &resto[..fim];
        let p = resto[fim + 1..]
            .strip_prefix(':')
            .ok_or_else(|| anyhow!("faltou a porta"))?;
        (h.to_string(), p)
    } else {
        // Primeiro ':' separa a porta: o resto nunca pode virar parte do host.
        let (h, p) = s.split_once(':').ok_or_else(|| anyhow!("faltou a porta (host:porta)"))?;
        (h.to_string(), p)
    };
    let p = p.trim_end_matches('/');
    // A mensagem nunca repete o que foi digitado: pode ser um pedaço da senha.
    let porta: u16 = p
        .parse()
        .map_err(|_| anyhow!("porta inválida (use host:porta, com usuário e senha antes do @)"))?;
    if porta == 0 {
        bail!("porta inválida: 0");
    }
    if !host_valido(&h, ipv6) {
        bail!("endereço inválido");
    }
    Ok((h, porta))
}

/// Lê um endereço colado pelo usuário. Formatos aceitos:
///
/// - `socks5://usuario:senha@host:porta` (também `socks5h://`, `socks://`)
/// - `http://usuario:senha@host:porta`
/// - `host:porta` (assume SOCKS5)
/// - `host:porta:usuario:senha` (formato comum de listas de provedores)
pub fn ler(texto: &str, nome: Option<&str>) -> Result<Saida> {
    let t = texto.trim();
    if t.is_empty() {
        bail!("cole o endereço da saída");
    }

    let (tipo, resto) = if let Some((esquema, resto)) = t.split_once("://") {
        let tipo = match esquema.to_ascii_lowercase().as_str() {
            "socks5" | "socks5h" | "socks" => Tipo::Socks5,
            "http" => Tipo::Http,
            "https" => bail!("proxy HTTPS ainda não é suportado; use http:// ou socks5://"),
            outro => bail!("tipo desconhecido: {outro}"),
        };
        (tipo, resto)
    } else {
        (Tipo::Socks5, t)
    };

    let (usuario, senha, endereco) = if let Some((cred, end)) = resto.rsplit_once('@') {
        let (u, s) = cred.split_once(':').unwrap_or((cred, ""));
        (
            Some(decodificar_pct(u)),
            if s.is_empty() { None } else { Some(decodificar_pct(s)) },
            end,
        )
    } else if !t.contains("://") && resto.matches(':').count() == 3 && !resto.starts_with('[') {
        // host:porta:usuario:senha
        let partes: Vec<&str> = resto.splitn(4, ':').collect();
        let end = format!("{}:{}", partes[0], partes[1]);
        let usuario = Some(partes[2].to_string()).filter(|u| !u.is_empty());
        let senha = Some(partes[3].to_string()).filter(|s| !s.is_empty());
        return montar(tipo, &end, usuario, senha, nome);
    } else {
        (None, None, resto)
    };

    montar(tipo, endereco, usuario.filter(|u| !u.is_empty()), senha, nome)
}

fn montar(tipo: Tipo, endereco: &str, usuario: Option<String>, senha: Option<String>, nome: Option<&str>) -> Result<Saida> {
    let (host, porta) = host_porta(endereco)?;
    if let Some(u) = &usuario {
        if u.len() > 255 {
            bail!("usuário longo demais");
        }
    }
    if let Some(s) = &senha {
        if s.len() > 255 {
            bail!("senha longa demais");
        }
    }
    let nome = nome
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| host.clone());
    Ok(Saida {
        id: novo_id(),
        nome,
        tipo,
        host,
        porta,
        usuario,
        senha,
        teste: None,
    })
}

// --- Código de convite -------------------------------------------------------

const PREFIXO: &str = "ponte1.";

#[derive(Serialize, Deserialize)]
struct ItemConvite {
    n: String,
    t: Tipo,
    h: String,
    p: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    u: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    s: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Convite {
    v: u8,
    s: Vec<ItemConvite>,
}

pub fn exportar(saidas: &[&Saida]) -> String {
    let c = Convite {
        v: 1,
        s: saidas
            .iter()
            .map(|s| ItemConvite {
                n: s.nome.clone(),
                t: s.tipo,
                h: s.host.clone(),
                p: s.porta,
                u: s.usuario.clone(),
                s: s.senha.clone(),
            })
            .collect(),
    };
    let json = serde_json::to_vec(&c).expect("convite serializável");
    format!("{PREFIXO}{}", URL_SAFE_NO_PAD.encode(json))
}

/// Aceita um código de convite ou uma lista de endereços, um por linha.
pub fn importar(texto: &str) -> Result<Vec<Saida>> {
    let t = texto.trim();
    if let Some(cod) = t.strip_prefix(PREFIXO) {
        let bytes = URL_SAFE_NO_PAD
            .decode(cod.trim().trim_end_matches('='))
            .context("código de convite corrompido")?;
        let c: Convite = serde_json::from_slice(&bytes).context("código de convite inválido")?;
        if c.v != 1 {
            bail!("código de convite de uma versão mais nova do programa");
        }
        let mut out = Vec::new();
        for it in c.s {
            let mut s = montar(it.t, &format!("{}:{}", fmt_host(&it.h), it.p), it.u, it.s, Some(&it.n))?;
            s.id = format!("{}{}", novo_id(), out.len());
            out.push(s);
        }
        if out.is_empty() {
            bail!("o convite não tem nenhuma saída");
        }
        return Ok(out);
    }

    let mut out = Vec::new();
    for (i, linha) in t.lines().map(str::trim).filter(|l| !l.is_empty()).enumerate() {
        let mut s = ler(linha, None).with_context(|| format!("linha {}", i + 1))?;
        s.id = format!("{}{}", novo_id(), i);
        out.push(s);
    }
    if out.is_empty() {
        bail!("nada para importar");
    }
    Ok(out)
}

fn fmt_host(h: &str) -> String {
    if h.contains(':') {
        format!("[{h}]")
    } else {
        h.to_string()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_url_socks_com_senha() {
        let s = ler("socks5://ana:se%40nha@200.1.2.3:1080", Some("Chile")).unwrap();
        assert_eq!(s.tipo, Tipo::Socks5);
        assert_eq!(s.host, "200.1.2.3");
        assert_eq!(s.porta, 1080);
        assert_eq!(s.usuario.as_deref(), Some("ana"));
        assert_eq!(s.senha.as_deref(), Some("se@nha"));
        assert_eq!(s.nome, "Chile");
    }

    #[test]
    fn le_http_e_formatos_curtos() {
        let s = ler("http://proxy.exemplo.com:8080", None).unwrap();
        assert_eq!(s.tipo, Tipo::Http);
        assert_eq!(s.nome, "proxy.exemplo.com");
        assert!(s.usuario.is_none());

        let s = ler("10.0.0.1:1080", None).unwrap();
        assert_eq!(s.tipo, Tipo::Socks5);

        let s = ler("10.0.0.1:1080:joao:123", None).unwrap();
        assert_eq!(s.usuario.as_deref(), Some("joao"));
        assert_eq!(s.senha.as_deref(), Some("123"));

        let s = ler("socks5://[2001:db8::1]:1080", None).unwrap();
        assert_eq!(s.host, "2001:db8::1");
    }

    #[test]
    fn recusa_lixo() {
        assert!(ler("", None).is_err());
        assert!(ler("semporta", None).is_err());
        assert!(ler("ftp://a:1", None).is_err());
        assert!(ler("host:99999", None).is_err());
        assert!(ler("evil host:1080", None).is_err());
    }

    #[test]
    fn senha_nunca_aparece_no_erro_nem_no_host() {
        // Achado da revisão: dois-pontos a mais viravam parte do host ou da mensagem.
        let e = ler("socks5://1.2.3.4:1080:joao:minhasenha", None).unwrap_err().to_string();
        assert!(!e.contains("minhasenha"), "{e}");
        assert!(ler("1.2.3.4:1080:joao:ab:12", None).is_err());
        let s = ler("1.2.3.4:1080::", None).unwrap();
        assert!(s.usuario.is_none() && s.senha.is_none());
    }

    #[test]
    fn convite_ida_e_volta() {
        let a = ler("socks5://u:p@1.2.3.4:1080", Some("VPS Chile")).unwrap();
        let cod = exportar(&[&a]);
        assert!(cod.starts_with("ponte1."));
        let v = importar(&cod).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].nome, "VPS Chile");
        assert_eq!(v[0].senha.as_deref(), Some("p"));
        assert!(v[0].mesmo_destino(&a));
    }

    #[test]
    fn importa_lista_de_linhas() {
        let v = importar("1.1.1.1:1080\nhttp://2.2.2.2:3128\n").unwrap();
        assert_eq!(v.len(), 2);
        assert_ne!(v[0].id, v[1].id);
    }
}
