//! Em que fase está a sessão do Discord e se ela nasceu corrigida.
//!
//! - **Abertura**: o Discord está nascendo; o que é dele sai pela saída escolhida.
//! - **Estabelecida**: a região já está gravada na sessão; tudo sai direto.
//!
//! A janela de abertura fecha 30 s depois da última conexão que decide a região,
//! com teto de 120 s (números medidos pelo FOL-discord numa abertura real).
//! Ela fica aberta enquanto o Discord está fechado, para não perder a corrida
//! com um Discord que abre rápido.
//!
//! **Modo estrito**: se nenhuma conexão decisiva conseguiu sair pela saída, a
//! janela não fecha. O Discord fica sem conectar em vez de nascer com a região
//! brasileira sem ninguém perceber.

use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const SILENCIO: Duration = Duration::from_secs(30);
pub const TETO: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fase {
    Abertura,
    Estabelecida,
}

/// O que a janela mostra sobre a sessão atual do Discord.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Correcao {
    /// O Discord já estava aberto quando o programa começou.
    Desconhecida,
    /// O Discord está fechado; vai ser corrigido quando abrir.
    Aguardando,
    /// O Discord está abrindo agora, pela saída.
    Abrindo,
    /// A sessão nasceu pela saída estrangeira.
    Corrigida,
    /// A sessão nasceu (ao menos em parte) pelo IP brasileiro.
    SemCorrecao,
    /// A saída falhou e o modo estrito está segurando o Discord.
    Bloqueada,
}

struct Interno {
    fase: Fase,
    inicio: Instant,
    ultima_decisiva: Option<Instant>,
    discord_aberto: bool,
    em_voo: u32,
    sucessos: u32,
    falhas: u32,
    diretas: u32,
    correcao: Correcao,
    saida_usada: Option<String>,
}

pub struct Sessao {
    i: Mutex<Interno>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Resumo {
    pub fase: Fase,
    pub correcao: Correcao,
    pub saida_usada: Option<String>,
    pub segundos_na_abertura: u64,
}

impl Sessao {
    pub fn nova(discord_ja_aberto: bool) -> Self {
        let agora = Instant::now();
        Sessao {
            i: Mutex::new(Interno {
                fase: if discord_ja_aberto { Fase::Estabelecida } else { Fase::Abertura },
                inicio: agora,
                ultima_decisiva: None,
                discord_aberto: discord_ja_aberto,
                em_voo: 0,
                sucessos: 0,
                falhas: 0,
                diretas: 0,
                correcao: if discord_ja_aberto { Correcao::Desconhecida } else { Correcao::Aguardando },
                saida_usada: None,
            }),
        }
    }

    fn com<R>(&self, f: impl FnOnce(&mut Interno) -> R) -> R {
        let mut g = self.i.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut g)
    }

    pub fn fase(&self) -> Fase {
        self.com(|i| i.fase)
    }

    pub fn resumo(&self) -> Resumo {
        self.com(|i| Resumo {
            fase: i.fase,
            correcao: i.correcao,
            saida_usada: i.saida_usada.clone(),
            segundos_na_abertura: if i.fase == Fase::Abertura && i.discord_aberto {
                i.inicio.elapsed().as_secs()
            } else {
                0
            },
        })
    }

    /// Uma sessão nova vai nascer: reabre a janela e zera a contagem.
    pub fn reabrir(&self, agora: Instant) {
        self.com(|i| {
            i.fase = Fase::Abertura;
            i.inicio = agora;
            i.ultima_decisiva = None;
            i.sucessos = 0;
            i.falhas = 0;
            i.diretas = 0;
            i.saida_usada = None;
            i.correcao = if i.discord_aberto { Correcao::Abrindo } else { Correcao::Aguardando };
        })
    }

    pub fn discord(&self, aberto: bool, agora: Instant) {
        self.com(|i| {
            if aberto && !i.discord_aberto && i.fase == Fase::Abertura {
                // O relógio e a contagem começam quando o Discord aparece. O que veio
                // antes (por exemplo, uma aba do navegador em discord.com, que também
                // passa pelo PAC do Windows) não diz nada sobre a sessão do Discord.
                i.inicio = agora;
                i.ultima_decisiva = None;
                i.sucessos = 0;
                i.falhas = 0;
                i.diretas = 0;
                i.saida_usada = None;
                i.correcao = Correcao::Abrindo;
            }
            i.discord_aberto = aberto;
        })
    }

    pub fn handshake_inicio(&self) {
        self.com(|i| i.em_voo += 1)
    }

    pub fn handshake_fim(&self) {
        self.com(|i| i.em_voo = i.em_voo.saturating_sub(1))
    }

    pub fn sucesso_decisivo(&self, saida: &str, agora: Instant) {
        self.com(|i| {
            if i.fase != Fase::Abertura || !i.discord_aberto {
                return;
            }
            i.sucessos += 1;
            i.ultima_decisiva = Some(agora);
            i.saida_usada = Some(saida.to_string());
            if i.correcao == Correcao::Bloqueada || i.correcao == Correcao::Aguardando {
                i.correcao = Correcao::Abrindo;
            }
        })
    }

    /// A saída falhou e o modo estrito recusou a conexão.
    pub fn falha_decisiva(&self) {
        self.com(|i| {
            if i.fase != Fase::Abertura || !i.discord_aberto {
                return;
            }
            i.falhas += 1;
            i.correcao = Correcao::Bloqueada;
        })
    }

    /// Uma conexão que decide a região saiu pelo IP brasileiro.
    pub fn direta_decisiva(&self, agora: Instant) {
        self.com(|i| {
            if i.fase != Fase::Abertura || !i.discord_aberto {
                return;
            }
            i.diretas += 1;
            i.ultima_decisiva = Some(agora);
        })
    }

    /// Chamado a cada segundo. Devolve `true` no instante em que a janela fecha.
    pub fn tick(&self, agora: Instant, estrito: bool, manter_pela_saida: bool) -> bool {
        self.com(|i| {
            if i.fase != Fase::Abertura {
                return false;
            }
            if !i.discord_aberto {
                i.inicio = agora;
                return false;
            }
            if manter_pela_saida && i.sucessos > 0 {
                // Modo "sempre pela saída": a fase nunca fecha, mas a sessão já está corrigida.
                if i.diretas == 0 {
                    i.correcao = Correcao::Corrigida;
                }
                return false;
            }
            let desde_inicio = agora.saturating_duration_since(i.inicio);
            if i.em_voo > 0 && desde_inicio < TETO + Duration::from_secs(15) {
                return false;
            }

            let fechar = if i.sucessos > 0 || i.diretas > 0 {
                let silencio = i
                    .ultima_decisiva
                    .map(|t| agora.saturating_duration_since(t) >= SILENCIO)
                    .unwrap_or(false);
                silencio || desde_inicio >= TETO
            } else if estrito {
                // Nada saiu pela saída ainda: segurar o Discord até ela responder.
                false
            } else {
                desde_inicio >= TETO
            };

            if fechar {
                i.fase = Fase::Estabelecida;
                i.correcao = if i.sucessos > 0 && i.diretas == 0 {
                    Correcao::Corrigida
                } else {
                    Correcao::SemCorrecao
                };
            }
            fechar
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn s(seg: u64, base: Instant) -> Instant {
        base + Duration::from_secs(seg)
    }

    #[test]
    fn fecha_depois_do_silencio_e_fica_corrigida() {
        let t0 = Instant::now();
        let ss = Sessao::nova(false);
        ss.discord(true, t0);
        ss.sucesso_decisivo("Chile", s(10, t0));
        assert!(!ss.tick(s(30, t0), true, false));
        assert!(ss.tick(s(41, t0), true, false));
        assert_eq!(ss.fase(), Fase::Estabelecida);
        assert_eq!(ss.resumo().correcao, Correcao::Corrigida);
    }

    #[test]
    fn nao_fecha_enquanto_o_discord_esta_fechado() {
        let t0 = Instant::now();
        let ss = Sessao::nova(false);
        assert!(!ss.tick(s(500, t0), false, false));
        assert_eq!(ss.fase(), Fase::Abertura);
        assert_eq!(ss.resumo().correcao, Correcao::Aguardando);
    }

    #[test]
    fn estrito_segura_sem_sucesso() {
        let t0 = Instant::now();
        let ss = Sessao::nova(false);
        ss.discord(true, t0);
        ss.falha_decisiva();
        assert!(!ss.tick(s(600, t0), true, false));
        assert_eq!(ss.resumo().correcao, Correcao::Bloqueada);
    }

    #[test]
    fn sem_estrito_fecha_no_teto_e_avisa() {
        let t0 = Instant::now();
        let ss = Sessao::nova(false);
        ss.discord(true, t0);
        ss.direta_decisiva(s(5, t0));
        assert!(ss.tick(s(40, t0), false, false));
        assert_eq!(ss.resumo().correcao, Correcao::SemCorrecao);
    }

    #[test]
    fn trafego_antes_do_discord_abrir_nao_conta() {
        // Achado da revisão: uma aba do navegador em discord.com, com o Discord
        // fechado, fazia a janela fechar no primeiro segundo como "Corrigida".
        let t0 = Instant::now();
        let ss = Sessao::nova(false);
        ss.sucesso_decisivo("Chile", s(1, t0));
        ss.discord(true, s(60, t0));
        assert!(!ss.tick(s(61, t0), true, false));
        assert_eq!(ss.fase(), Fase::Abertura);
        assert_eq!(ss.resumo().correcao, Correcao::Abrindo);
    }

    #[test]
    fn discord_ja_aberto_comeca_desconhecido() {
        let ss = Sessao::nova(true);
        assert_eq!(ss.fase(), Fase::Estabelecida);
        assert_eq!(ss.resumo().correcao, Correcao::Desconhecida);
    }

    #[test]
    fn manter_pela_saida_nunca_fecha() {
        let t0 = Instant::now();
        let ss = Sessao::nova(false);
        ss.discord(true, t0);
        ss.sucesso_decisivo("Chile", s(1, t0));
        assert!(!ss.tick(s(5000, t0), true, true));
        assert_eq!(ss.fase(), Fase::Abertura);
        assert_eq!(ss.resumo().correcao, Correcao::Corrigida);
    }
}
