//! Decide, por host, se a conexão sai pela saída estrangeira ou direto.
//!
//! Adaptado do FOL-discord (MIT, github.com/schacal/FOL-discord, src/routing.rs).
//!
//! Duas listas, com papéis diferentes:
//!
//! - `DESVIADOS` diz o que sai por fora enquanto a sessão está nascendo. É
//!   sempre uma lista de domínios do Discord, nunca "qualquer host": o proxy
//!   local aceita conexão de qualquer programa da máquina e não pode virar um
//!   relay estrangeiro de uso geral.
//! - `DECIDEM_REGIAO` diz o que alimenta o relógio que fecha a janela de
//!   abertura: só os hosts cujo IP de origem decide a região da sessão.
//!
//! O TCP dos servidores de voz (`c-*.discord.media`) sai direto sempre: não
//! decide região e prendê-lo na saída só pagaria latência. Áudio, câmera e
//! tela viajam em UDP e nunca passam pelo proxy.

const DESVIADOS: &[&str] = &["discord.com", "discordapp.com", "discord.gg", "latency.discord.media"];
const DECIDEM_REGIAO: &[&str] = &["discord.com", "gateway.discord.gg", "latency.discord.media"];
const AVISOS: &str = "status.discord.com";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rota {
    Exterior,
    Direta,
}

fn casa(host: &str, dominio: &str) -> bool {
    host == dominio || host.ends_with(&format!(".{dominio}"))
}

fn normalizar(host: &str) -> String {
    host.trim_end_matches('.').to_ascii_lowercase()
}

/// Só o que um nome DNS pode ter. Barra truques como `evil.com\0.discord.com`.
pub fn nome_bem_formado(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

pub fn decidir(host: &str, em_abertura: bool) -> Rota {
    if !em_abertura {
        return Rota::Direta;
    }
    let host = normalizar(host);
    if !nome_bem_formado(&host) {
        return Rota::Direta;
    }
    if DESVIADOS.iter().any(|d| casa(&host, d)) {
        Rota::Exterior
    } else {
        Rota::Direta
    }
}

/// `gateway.discord.gg` e os sabores regionais (`gateway-us-east1-b.discord.gg`).
/// O `remote-auth-gateway` (login por QR code) não é o gateway da sessão.
pub fn e_gateway(host: &str) -> bool {
    let host = normalizar(host);
    casa(&host, "discord.gg")
        && host
            .split('.')
            .next()
            .is_some_and(|r| r.starts_with("gateway"))
}

/// Se o IP de origem desta conexão decide a região da sessão.
pub fn decide_regiao(host: &str) -> bool {
    let host = normalizar(host);
    if casa(&host, AVISOS) {
        return false;
    }
    DECIDEM_REGIAO.iter().any(|d| casa(&host, d)) || e_gateway(&host)
}

/// Nome amigável para a lista de atividade.
pub fn descrever(host: &str) -> &'static str {
    let h = normalizar(host);
    if e_gateway(&h) {
        "Conexão principal (gateway)"
    } else if casa(&h, "latency.discord.media") {
        "Escolha de região"
    } else if h.starts_with("c-") && casa(&h, "discord.media") {
        "Servidor de voz"
    } else if casa(&h, "cdn.discordapp.com") || casa(&h, "media.discordapp.net") {
        "Imagens e arquivos"
    } else if casa(&h, "status.discord.com") {
        "Página de status"
    } else if casa(&h, "discord.com") {
        "API do Discord"
    } else {
        "Discord"
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn na_abertura_o_discord_sai_por_fora() {
        for h in [
            "discord.com",
            "gateway.discord.gg",
            "gateway-us-east1-b.discord.gg",
            "latency.discord.media",
            "cdn.discordapp.com",
            "DISCORD.COM.",
        ] {
            assert_eq!(decidir(h, true), Rota::Exterior, "{h}");
        }
    }

    #[test]
    fn voz_vai_direto_mesmo_na_abertura() {
        for h in ["c-gru17-851904d3.discord.media", "discord.media"] {
            assert_eq!(decidir(h, true), Rota::Direta, "{h}");
        }
    }

    #[test]
    fn resto_da_internet_vai_direto() {
        for h in ["google.com", "discord.com.evil.net", "naodiscord.com", "meudiscord.gg", "media.discordapp.net"] {
            assert_eq!(decidir(h, true), Rota::Direta, "{h}");
        }
    }

    #[test]
    fn nome_mal_formado_nunca_sai_por_fora() {
        for h in ["evil.com\0.discord.com", "evil.com\n.discord.com", "a b.discord.com", ""] {
            assert_eq!(decidir(h, true), Rota::Direta, "{h:?}");
        }
    }

    #[test]
    fn sessao_estabelecida_vai_tudo_direto() {
        assert_eq!(decidir("discord.com", false), Rota::Direta);
        assert_eq!(decidir("gateway.discord.gg", false), Rota::Direta);
    }

    #[test]
    fn so_quem_decide_regiao_alimenta_o_relogio() {
        for h in ["discord.com", "gateway.discord.gg", "gateway-us-east1-b.discord.gg", "latency.discord.media"] {
            assert!(decide_regiao(h), "{h}");
            assert_eq!(decidir(h, true), Rota::Exterior, "{h} precisa ser desviado");
        }
        for h in ["cdn.discordapp.com", "status.discord.com", "remote-auth-gateway.discord.gg", "google.com"] {
            assert!(!decide_regiao(h), "{h}");
        }
    }
}
