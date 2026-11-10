//! O estado do programa e o laço principal: proxy local, PAC, vigia do Discord
//! e o relógio da janela de abertura.

use crate::piscina::{EstadoPiscina, Piscina, ID_AUTO};
use crate::saidas::{Saida, SaidaPublica};
use crate::sessao::{Correcao, Fase, Resumo, Sessao};
use crate::{discord, pac, proxy_local, rotas, vigia};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

pub const PORTA_SOCKS: u16 = 9350;
pub const PORTA_PAC: u16 = 9371;

fn sim() -> bool {
    true
}

fn auto() -> Option<String> {
    Some(ID_AUTO.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub saidas: Vec<Saida>,
    /// Id da saída manual escolhida, ou `"auto"` para a piscina de proxies públicos.
    #[serde(default = "auto")]
    pub escolhida: Option<String>,
    /// Se a saída falhar, segurar o Discord em vez de abrir com o IP brasileiro.
    #[serde(default = "sim")]
    pub estrito: bool,
    #[serde(default)]
    pub pausado: bool,
    /// Manter as conexões que decidem a região sempre pela saída.
    /// Ligado de fábrica: testado em 09/11/2026, o Discord confere o IP de novo
    /// quando a câmera é ligada; só a abertura pela saída não basta.
    #[serde(default = "sim")]
    pub manter_pela_saida: bool,
    /// Ao iniciar com o PC, reabrir o Discord se ele abriu antes do programa.
    #[serde(default = "sim")]
    pub reabrir_discord_no_inicio: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            saidas: vec![],
            escolhida: auto(),
            estrito: true,
            pausado: false,
            manter_pela_saida: true,
            reabrir_discord_no_inicio: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TipoAtividade {
    Exterior,
    Direto,
    Bloqueado,
    Info,
    Aviso,
}

#[derive(Clone, Debug, Serialize)]
pub struct Atividade {
    pub quando: u64,
    pub tipo: TipoAtividade,
    pub host: String,
    pub porta: u16,
    pub descricao: String,
    pub detalhe: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TrocaDeProxy {
    pub de: SaidaPublica,
    pub para: Option<SaidaPublica>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Estado {
    pub versao: &'static str,
    pub sessao: Resumo,
    pub pausado: bool,
    pub discord_aberto: bool,
    pub discord_instalado: bool,
    pub pac_ativo: bool,
    pub porta_socks: u16,
    pub saidas: Vec<SaidaPublica>,
    pub escolhida: Option<String>,
    /// A saída em uso agora (na automática, o melhor proxy da piscina).
    pub em_uso: Option<SaidaPublica>,
    /// Coordenada aproximada da saída em uso (centro do país), para o globo.
    pub coord_saida: Option<(f64, f64)>,
    pub auto: bool,
    pub piscina: EstadoPiscina,
    pub estrito: bool,
    pub manter_pela_saida: bool,
    pub reabrir_discord_no_inicio: bool,
    pub atividade: Vec<Atividade>,
    pub contagem_exterior: u32,
    pub contagem_direto: u32,
    pub contagem_bloqueado: u32,
    pub aviso: Option<String>,
}

pub struct Nucleo {
    pub sessao: Sessao,
    pub derrubar: Notify,
    pub piscina: Arc<Piscina>,
    config: Mutex<Config>,
    caminho_config: PathBuf,
    atividade: Mutex<VecDeque<Atividade>>,
    discord_aberto: AtomicBool,
    discord_exe: Mutex<Option<String>>,
    porta_socks: AtomicU16,
    porta_pac: AtomicU16,
    aviso: Mutex<Option<String>>,
    reiniciando: AtomicBool,
    /// (id do proxy que caiu, quando) do último alerta de troca.
    ultimo_alerta: Mutex<Option<(String, Instant)>>,
    app: AppHandle,
}

pub fn agora_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Nucleo {
    pub fn novo(pasta: PathBuf, app: AppHandle) -> Self {
        let caminho_config = pasta.join("config.json");
        let config = std::fs::read(&caminho_config)
            .ok()
            .and_then(|b| serde_json::from_slice::<Config>(&b).ok())
            .unwrap_or_default();

        let mut leitor = vigia::Leitor::novo();
        let ja_aberto = leitor.exe_aberto();

        Nucleo {
            sessao: Sessao::nova(ja_aberto.is_some()),
            derrubar: Notify::new(),
            piscina: Arc::new(Piscina::nova()),
            config: Mutex::new(config),
            caminho_config,
            atividade: Mutex::new(VecDeque::new()),
            discord_aberto: AtomicBool::new(ja_aberto.is_some()),
            discord_exe: Mutex::new(ja_aberto),
            porta_socks: AtomicU16::new(0),
            porta_pac: AtomicU16::new(0),
            aviso: Mutex::new(None),
            reiniciando: AtomicBool::new(false),
            ultimo_alerta: Mutex::new(None),
            app,
        }
    }

    // --- Config ---------------------------------------------------------------

    pub fn com_config<R>(&self, f: impl FnOnce(&mut Config) -> R) -> R {
        let mut g = self.config.lock().unwrap_or_else(|e| e.into_inner());
        let r = f(&mut g);
        if let Ok(json) = serde_json::to_vec_pretty(&*g) {
            let tmp = self.caminho_config.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &self.caminho_config);
            }
        }
        r
    }

    fn ler_config<R>(&self, f: impl FnOnce(&Config) -> R) -> R {
        let g = self.config.lock().unwrap_or_else(|e| e.into_inner());
        f(&g)
    }

    pub fn pausado(&self) -> bool {
        self.ler_config(|c| c.pausado)
    }
    pub fn estrito(&self) -> bool {
        self.ler_config(|c| c.estrito)
    }
    pub fn manter_pela_saida(&self) -> bool {
        self.ler_config(|c| c.manter_pela_saida)
    }

    pub fn usando_auto(&self) -> bool {
        self.ler_config(|c| c.escolhida.as_deref() == Some(ID_AUTO))
    }

    /// Há uma saída escolhida (automática, ou manual que ainda existe na lista)?
    pub fn saida_configurada(&self) -> bool {
        self.ler_config(|c| match c.escolhida.as_deref() {
            Some(ID_AUTO) => true,
            Some(id) => c.saidas.iter().any(|s| s.id == id),
            None => false,
        })
    }

    /// A saída que está valendo agora (na automática, o melhor da piscina).
    pub fn saida_escolhida(&self) -> Option<Saida> {
        if self.usando_auto() {
            return self.piscina.melhores(1).into_iter().next();
        }
        self.ler_config(|c| {
            let id = c.escolhida.as_ref()?;
            c.saidas.iter().find(|s| &s.id == id).cloned()
        })
    }

    /// Na automática, duas tentativas em proxies diferentes; na manual, uma.
    pub fn saidas_para_tentar(&self) -> Vec<Saida> {
        if self.usando_auto() {
            self.piscina.melhores(2)
        } else {
            self.saida_escolhida().into_iter().collect()
        }
    }

    /// Baixa as listas e valida, em segundo plano.
    pub fn atualizar_piscina(self: &Arc<Self>) {
        let n = self.clone();
        tauri::async_runtime::spawn(async move {
            n.info(TipoAtividade::Info, "Buscando proxies nas listas públicas");
            n.emitir();
            match n.piscina.atualizar().await {
                Ok(q) if q > 0 => n.info(TipoAtividade::Info, format!("{q} proxies funcionando fora do Brasil")),
                Ok(_) => n.info(TipoAtividade::Aviso, "Nenhum proxy público passou no teste agora; tento de novo em instantes"),
                Err(e) => n.info(TipoAtividade::Aviso, format!("Listas de proxies: {e}")),
            }
            n.emitir();
        });
    }

    pub fn saida(&self, id: &str) -> Option<Saida> {
        self.ler_config(|c| c.saidas.iter().find(|s| s.id == id).cloned())
    }

    // --- Atividade --------------------------------------------------------------

    pub fn registrar(&self, tipo: TipoAtividade, host: &str, porta: u16, detalhe: Option<String>) {
        let a = Atividade {
            quando: agora_ms(),
            tipo,
            host: host.to_string(),
            porta,
            descricao: rotas::descrever(host).to_string(),
            detalhe,
        };
        let mut g = self.atividade.lock().unwrap_or_else(|e| e.into_inner());
        g.push_front(a);
        g.truncate(200);
    }

    pub fn info(&self, tipo: TipoAtividade, texto: impl Into<String>) {
        let a = Atividade {
            quando: agora_ms(),
            tipo,
            host: String::new(),
            porta: 0,
            descricao: texto.into(),
            detalhe: None,
        };
        let mut g = self.atividade.lock().unwrap_or_else(|e| e.into_inner());
        g.push_front(a);
        g.truncate(200);
    }

    /// O proxy em uso falhou e foi trocado (ou não havia substituto). Avisa a
    /// interface uma vez por proxy, no máximo a cada 20 s, para não virar spam
    /// quando várias conexões caem juntas.
    pub fn alertar_troca(&self, de: SaidaPublica, para: Option<SaidaPublica>) {
        {
            let mut ultimo = self.ultimo_alerta.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((id, quando)) = ultimo.as_ref() {
                let mesmo = *id == de.id && quando.elapsed() < Duration::from_secs(300);
                if mesmo || quando.elapsed() < Duration::from_secs(20) {
                    return;
                }
            }
            *ultimo = Some((de.id.clone(), Instant::now()));
        }
        let texto = match &para {
            Some(p) => format!("Proxy {} parou de responder; trocado para {}", de.host, p.host),
            None => format!("Proxy {} parou de responder e nenhum outro respondeu; buscando novos", de.host),
        };
        self.info(TipoAtividade::Aviso, texto);
        let _ = self.app.emit("troca_de_proxy", TrocaDeProxy { de, para });
    }

    pub fn definir_aviso(&self, aviso: Option<String>) {
        *self.aviso.lock().unwrap_or_else(|e| e.into_inner()) = aviso;
    }

    // --- Estado para a interface -----------------------------------------------

    pub fn estado(&self) -> Estado {
        let (saidas, escolhida, pausado, estrito, manter, reabrir) = self.ler_config(|c| {
            (
                c.saidas.iter().map(|s| s.publica()).collect::<Vec<_>>(),
                c.escolhida.clone(),
                c.pausado,
                c.estrito,
                c.manter_pela_saida,
                c.reabrir_discord_no_inicio,
            )
        });
        let atividade: Vec<Atividade> = {
            let g = self.atividade.lock().unwrap_or_else(|e| e.into_inner());
            g.iter().cloned().collect()
        };
        let conta = |t: TipoAtividade| atividade.iter().filter(|a| a.tipo == t).count() as u32;
        Estado {
            versao: env!("CARGO_PKG_VERSION"),
            sessao: self.sessao.resumo(),
            pausado,
            discord_aberto: self.discord_aberto.load(Ordering::Relaxed),
            discord_instalado: discord::instalado(),
            pac_ativo: pac::ativo(),
            porta_socks: self.porta_socks.load(Ordering::Relaxed),
            contagem_exterior: conta(TipoAtividade::Exterior),
            contagem_direto: conta(TipoAtividade::Direto),
            contagem_bloqueado: conta(TipoAtividade::Bloqueado),
            atividade: atividade.into_iter().take(60).collect(),
            em_uso: self.saida_escolhida().map(|s| s.publica()),
            coord_saida: self
                .saida_escolhida()
                .and_then(|s| s.teste)
                .and_then(|t| t.pais)
                .and_then(|p| crate::geo::centro_pais(&p)),
            auto: escolhida.as_deref() == Some(ID_AUTO),
            piscina: self.piscina.estado(),
            saidas,
            escolhida,
            estrito,
            manter_pela_saida: manter,
            reabrir_discord_no_inicio: reabrir,
            aviso: self.aviso.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        }
    }

    pub fn emitir(&self) {
        let _ = self.app.emit("estado", self.estado());
    }

    // --- PAC --------------------------------------------------------------------

    pub fn aplicar_pac(&self) {
        let porta = self.porta_pac.load(Ordering::Relaxed);
        if porta == 0 {
            return;
        }
        if self.pausado() {
            if let Err(e) = pac::desativar() {
                self.info(TipoAtividade::Aviso, format!("não consegui desligar o proxy automático: {e}"));
            }
            return;
        }
        match pac::ativar(&pac::url(porta)) {
            Ok(Some(anterior)) => self.info(
                TipoAtividade::Aviso,
                format!("havia outro proxy automático configurado ({anterior}); ele foi guardado e volta quando você pausar ou sair"),
            ),
            Ok(None) => {}
            Err(e) => self.info(TipoAtividade::Aviso, format!("não consegui ligar o proxy automático: {e}")),
        }
    }

    pub fn encerrar(&self) {
        let _ = pac::desativar();
    }

    // --- Discord ------------------------------------------------------------------

    pub async fn reiniciar_discord(self: &Arc<Self>) -> Result<(), String> {
        if self.reiniciando.swap(true, Ordering::SeqCst) {
            return Err("o Discord já está sendo reiniciado".into());
        }
        let r = self.reiniciar_discord_interno().await;
        self.reiniciando.store(false, Ordering::SeqCst);
        self.emitir();
        r
    }

    async fn reiniciar_discord_interno(self: &Arc<Self>) -> Result<(), String> {
        let ultimo = self.discord_exe.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let exe = discord::escolher(ultimo.as_deref()).ok_or("não encontrei o Discord instalado neste PC")?;

        // Garante o PAC no lugar antes de o Discord nascer.
        self.aplicar_pac();
        self.sessao.reabrir(Instant::now());
        self.info(TipoAtividade::Info, "Reiniciando o Discord");

        tokio::task::spawn_blocking(discord::fechar_todos).await.ok();
        // Espera os processos sumirem (até ~8 s).
        for _ in 0..16 {
            let algum = tokio::task::spawn_blocking(|| vigia::Leitor::novo().algum_aberto())
                .await
                .unwrap_or(false);
            if !algum {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        self.sessao.reabrir(Instant::now());
        discord::abrir(&exe).map_err(|e| e.to_string())?;
        self.info(TipoAtividade::Info, "Discord aberto; aguardando a sessão nascer pela saída");
        Ok(())
    }

    fn ao_evento(&self, ev: vigia::Evento) {
        let agora = Instant::now();
        match ev {
            vigia::Evento::Abriu { exe } => {
                *self.discord_exe.lock().unwrap_or_else(|e| e.into_inner()) = Some(exe);
                self.discord_aberto.store(true, Ordering::Relaxed);
                if self.sessao.fase() == Fase::Estabelecida && !self.reiniciando.load(Ordering::Relaxed) {
                    // Um Discord novo nasceu antes de percebermos o antigo morrer.
                    self.sessao.reabrir(agora);
                }
                self.sessao.discord(true, agora);
                self.info(TipoAtividade::Info, "Discord abriu");
            }
            vigia::Evento::Fechou => {
                self.discord_aberto.store(false, Ordering::Relaxed);
                self.sessao.discord(false, agora);
                self.sessao.reabrir(agora);
                self.info(TipoAtividade::Info, "Discord fechou");
            }
        }
    }
}

/// Sobe tudo e roda o relógio. Nunca retorna.
pub async fn iniciar(n: Arc<Nucleo>, iniciado_com_o_pc: bool) {
    match proxy_local::iniciar(n.clone(), PORTA_SOCKS).await {
        Ok(p) => n.porta_socks.store(p, Ordering::Relaxed),
        Err(e) => {
            n.definir_aviso(Some(format!("O proxy local não subiu: {e}")));
            n.emitir();
            return;
        }
    }
    let porta_socks = n.porta_socks.load(Ordering::Relaxed);
    match pac::servir(PORTA_PAC, porta_socks).await {
        Ok(p) => n.porta_pac.store(p, Ordering::Relaxed),
        Err(e) => {
            n.definir_aviso(Some(format!("O servidor do PAC não subiu: {e}")));
            n.emitir();
            return;
        }
    }
    n.aplicar_pac();
    n.info(TipoAtividade::Info, format!("Live Ponte no ar (proxy local na porta {porta_socks})"));

    // Sempre lista fresca a cada início.
    if n.usando_auto() {
        n.atualizar_piscina();
    }

    let ja_aberto = n.discord_aberto.load(Ordering::Relaxed);
    let inicial = n.discord_exe.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    vigia::iniciar(inicial, tx);

    if ja_aberto && !n.pausado() {
        let tem_saida = n.saida_configurada();
        let reabrir = n.ler_config(|c| c.reabrir_discord_no_inicio);
        if iniciado_com_o_pc && tem_saida && reabrir {
            n.info(TipoAtividade::Info, "O Discord abriu antes do Live Ponte; reabrindo pela saída");
            let n2 = n.clone();
            tauri::async_runtime::spawn(async move {
                // Na automática, espera a piscina ter alguém (até ~3 min).
                for _ in 0..180 {
                    if n2.saida_escolhida().is_some() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                let _ = n2.reiniciar_discord().await;
            });
        } else {
            n.info(
                TipoAtividade::Aviso,
                "O Discord já estava aberto: reinicie-o para que a sessão nasça pela saída",
            );
        }
    }

    let mut relogio = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            Some(ev) = rx.recv() => n.ao_evento(ev),
            _ = relogio.tick() => {
                let (estrito, manter) = n.ler_config(|c| (c.estrito, c.manter_pela_saida));
                let tem_saida = n.saida_configurada() && !n.pausado();
                if n.usando_auto() && !n.pausado() && n.piscina.deve_reabastecer() {
                    n.atualizar_piscina();
                }
                if n.sessao.tick(Instant::now(), estrito && tem_saida, manter) {
                    n.derrubar.notify_waiters();
                    let r = n.sessao.resumo();
                    match r.correcao {
                        Correcao::Corrigida => n.info(
                            TipoAtividade::Info,
                            format!(
                                "Sessão aberta pela saída {}. Tela e câmera devem estar liberadas; o resto volta a sair direto",
                                r.saida_usada.unwrap_or_default()
                            ),
                        ),
                        _ => n.info(
                            TipoAtividade::Aviso,
                            "A sessão nasceu sem correção (sem saída escolhida ou a saída falhou). Reinicie o Discord",
                        ),
                    }
                }
            }
        }
        n.emitir();
    }
}
