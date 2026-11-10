//! Saída automática: uma piscina de proxies SOCKS5 públicos, baixada de listas
//! abertas a cada início do programa e validada contra o próprio Discord.
//!
//! Um candidato só entra se responder e se o Discord **não** o colocar na
//! região `brazil` (mesmo critério do FOL-discord, src/pool.rs). Quem falha em
//! uso é rebaixado; duas falhas eliminam. Abaixo de `MINIMO`, reabastece.
//!
//! Proxies públicos são de desconhecidos: a conexão com o Discord é TLS, então
//! eles não leem conteúdo nem senha, mas veem que o Discord está sendo usado.

use crate::geo::{self, CandidatoGeo, Local};
use crate::saidas::{ResultadoTeste, Saida, SaidaPublica, Tipo};
use crate::teste;
use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub const ID_AUTO: &str = "auto";

#[derive(Clone, Copy)]
enum Formato {
    /// `proxies.json` do monosans, com latitude/longitude.
    Monosans,
    /// `data.json` do proxifly, com país.
    Proxifly,
    /// `ip:porta` por linha, sem geografia.
    Texto,
}

/// Listas públicas, atualizadas pelos mantenedores. As com geografia vêm
/// primeiro: são elas que permitem testar os proxies mais próximos antes.
const FONTES: &[(&str, &str, Formato)] = &[
    ("raw.githubusercontent.com", "/monosans/proxy-list/main/proxies.json", Formato::Monosans),
    ("raw.githubusercontent.com", "/proxifly/free-proxy-list/main/proxies/protocols/socks5/data.json", Formato::Proxifly),
    ("raw.githubusercontent.com", "/TheSpeedX/PROXY-List/master/socks5.txt", Formato::Texto),
    (
        "api.proxyscrape.com",
        "/v4/free-proxy-list/get?request=display_proxies&protocol=socks5&proxy_format=ipport&format=text",
        Formato::Texto,
    ),
];

/// Quantos proxies aprovados guardar. Os primeiros já ficam disponíveis
/// enquanto a busca continua.
const ALVO: usize = 25;
pub const MINIMO: usize = 3;
const PARALELO: usize = 96;
const MAX_TESTES: usize = 2000;
const PRAZO_TOTAL: Duration = Duration::from_secs(150);
const INTERVALO_MINIMO: Duration = Duration::from_secs(30);

struct Membro {
    saida: Saida,
    falhas: u32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct EstadoPiscina {
    pub atualizando: bool,
    pub saudaveis: usize,
    pub candidatos: usize,
    pub testados: usize,
    pub ultima_atualizacao: Option<u64>,
    pub erro: Option<String>,
    pub em_uso: Option<SaidaPublica>,
    /// Todos os proxies aprovados, na ordem de uso (o primeiro é o atual).
    pub membros: Vec<SaidaPublica>,
    /// Onde o usuário está, para a busca por proximidade (ex.: "BR · GIG").
    pub local: Option<String>,
    /// Coordenada aproximada do usuário (lat, lon), para o globo da interface.
    pub local_coord: Option<(f64, f64)>,
}

pub struct Piscina {
    membros: Mutex<Vec<Membro>>,
    info: Mutex<EstadoPiscina>,
    atualizando: AtomicBool,
    ultima_tentativa: Mutex<Option<Instant>>,
}

fn travar<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Piscina {
    pub fn nova() -> Self {
        Piscina {
            membros: Mutex::new(Vec::new()),
            info: Mutex::new(EstadoPiscina::default()),
            atualizando: AtomicBool::new(false),
            ultima_tentativa: Mutex::new(None),
        }
    }

    pub fn melhores(&self, n: usize) -> Vec<Saida> {
        travar(&self.membros).iter().take(n).map(|m| m.saida.clone()).collect()
    }

    pub fn tamanho(&self) -> usize {
        travar(&self.membros).len()
    }

    pub fn funcionou(&self, id: &str) {
        if let Some(m) = travar(&self.membros).iter_mut().find(|m| m.saida.id == id) {
            m.falhas = 0;
        }
    }

    /// O usuário escolheu este proxy: vai para a frente da fila.
    pub fn promover(&self, id: &str) -> bool {
        let mut g = travar(&self.membros);
        match g.iter().position(|m| m.saida.id == id) {
            Some(pos) => {
                let mut m = g.remove(pos);
                m.falhas = 0;
                g.insert(0, m);
                true
            }
            None => false,
        }
    }

    /// Rebaixa para o fim da fila; na segunda falha seguida, elimina.
    pub fn falhou(&self, id: &str) {
        let mut g = travar(&self.membros);
        if let Some(pos) = g.iter().position(|m| m.saida.id == id) {
            let mut m = g.remove(pos);
            m.falhas += 1;
            if m.falhas < 2 {
                g.push(m);
            }
        }
    }

    pub fn estado(&self) -> EstadoPiscina {
        let mut e = travar(&self.info).clone();
        let g = travar(&self.membros);
        e.saudaveis = g.len();
        e.em_uso = g.first().map(|m| m.saida.publica());
        e.membros = g.iter().map(|m| m.saida.publica()).collect();
        e.atualizando = self.atualizando.load(Ordering::Relaxed);
        e
    }

    pub fn atualizando(&self) -> bool {
        self.atualizando.load(Ordering::Relaxed)
    }

    /// Precisa reabastecer e já passou o intervalo mínimo desde a última tentativa.
    pub fn deve_reabastecer(&self) -> bool {
        if self.atualizando() || self.tamanho() >= MINIMO {
            return false;
        }
        travar(&self.ultima_tentativa).map_or(true, |t| t.elapsed() >= INTERVALO_MINIMO)
    }

    fn inserir(&self, saida: Saida) {
        let mut g = travar(&self.membros);
        if g.iter().any(|m| m.saida.host == saida.host && m.saida.porta == saida.porta) {
            return;
        }
        let ping = |s: &Saida| s.teste.as_ref().and_then(|t| t.ping_ms).unwrap_or(u32::MAX);
        let p = ping(&saida);
        let pos = g.iter().position(|m| ping(&m.saida) > p).unwrap_or(g.len());
        g.insert(pos, Membro { saida, falhas: 0 });
    }

    /// Baixa as listas e valida candidatos até ter `ALVO` saudáveis.
    pub async fn atualizar(self: &Arc<Self>) -> Result<usize> {
        if self.atualizando.swap(true, Ordering::SeqCst) {
            return Ok(self.tamanho());
        }
        *travar(&self.ultima_tentativa) = Some(Instant::now());
        {
            let mut i = travar(&self.info);
            i.erro = None;
            i.testados = 0;
            i.candidatos = 0;
        }
        let r = self.atualizar_interno().await;
        {
            let mut i = travar(&self.info);
            i.ultima_atualizacao = Some(crate::nucleo::agora_ms());
            if let Err(e) = &r {
                i.erro = Some(e.to_string());
            } else if self.tamanho() == 0 {
                i.erro = Some("nenhum proxy público passou no teste agora".into());
            }
        }
        self.atualizando.store(false, Ordering::SeqCst);
        r
    }

    async fn atualizar_interno(self: &Arc<Self>) -> Result<usize> {
        // Onde o usuário está e as listas, em paralelo.
        let (local, candidatos) = tokio::join!(meu_local(), baixar_listas());
        let mut candidatos = candidatos?;
        {
            let mut i = travar(&self.info);
            i.local = local.as_ref().map(|l| match &l.colo {
                Some(c) => format!("{} · {c}", l.pais),
                None => l.pais.clone(),
            });
            i.local_coord = local.as_ref().map(|l| l.coord);
        }
        {
            let ja: std::collections::HashSet<(String, u16)> = travar(&self.membros)
                .iter()
                .map(|m| (m.saida.host.clone(), m.saida.porta))
                .collect();
            candidatos.retain(|c| !ja.contains(&(c.host.clone(), c.porta)));
        }
        // Embaralha (para variar entre execuções) e depois ordena por distância:
        // a ordenação é estável, então quem não tem posição fica embaralhado no fim.
        embaralhar(&mut candidatos);
        geo::ordenar_por_proximidade(&mut candidatos, local.as_ref());
        candidatos.truncate(MAX_TESTES);
        travar(&self.info).candidatos = candidatos.len();

        let inicio = Instant::now();
        let sem = Arc::new(Semaphore::new(PARALELO));
        let mut tarefas = JoinSet::new();
        let mut fila = candidatos.into_iter();

        loop {
            // Mantém o paralelismo cheio enquanto há candidatos e tempo.
            while tarefas.len() < PARALELO && inicio.elapsed() < PRAZO_TOTAL && self.tamanho() < ALVO {
                let Some(c) = fila.next() else { break };
                let permissao = sem.clone().acquire_owned().await?;
                tarefas.spawn(async move {
                    let _p = permissao;
                    validar(c).await
                });
            }
            let Some(r) = tarefas.join_next().await else { break };
            travar(&self.info).testados += 1;
            if let Ok(Some(saida)) = r {
                self.inserir(saida);
            }
            if self.tamanho() >= ALVO || inicio.elapsed() >= PRAZO_TOTAL {
                break; // soltar o JoinSet aborta o que sobrou
            }
        }
        Ok(self.tamanho())
    }
}

/// Testa um candidato: abre túnel até latency.discord.media e lê a região.
async fn validar(c: CandidatoGeo) -> Option<Saida> {
    let mut s = Saida {
        id: format!("pub-{}-{}", c.host, c.porta),
        nome: String::new(),
        tipo: Tipo::Socks5,
        host: c.host,
        porta: c.porta,
        usuario: None,
        senha: None,
        teste: None,
    };
    let t0 = Instant::now();
    let corpo = tokio::time::timeout(
        Duration::from_secs(9),
        teste::get(Some(&s), "latency.discord.media", "/rtc"),
    )
    .await
    .ok()?
    .ok()?;
    let regiao = teste::primeira_regiao(&corpo)?;
    if regiao == "brazil" {
        return None;
    }
    let ping = t0.elapsed().as_millis() as u32;
    s.nome = format!("Pública · {}", nome_regiao(&regiao));
    s.teste = Some(ResultadoTeste {
        ok: true,
        quando: crate::nucleo::agora_ms(),
        ping_ms: Some(ping),
        ip: Some(s.host.clone()),
        pais: c.pais,
        regiao: Some(regiao),
        erro: None,
    });
    Some(s)
}

fn nome_regiao(r: &str) -> String {
    let mut c = r.chars();
    match c.next() {
        Some(p) => p.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// País e data center da Cloudflare que atendeu esta máquina (conexão direta).
async fn meu_local() -> Option<Local> {
    let trace = tokio::time::timeout(
        Duration::from_secs(8),
        teste::get(None, "one.one.one.one", "/cdn-cgi/trace"),
    )
    .await
    .ok()?
    .ok()?;
    geo::local_do_trace(&trace)
}

async fn baixar_listas() -> Result<Vec<CandidatoGeo>> {
    let mut tarefas = JoinSet::new();
    for &(host, caminho, formato) in FONTES {
        tarefas.spawn(async move { (formato, teste::get(None, host, caminho).await) });
    }
    let mut lidos: Vec<CandidatoGeo> = Vec::new();
    let mut erros = 0;
    while let Some(r) = tarefas.join_next().await {
        match r {
            Ok((formato, Ok(texto))) => lidos.extend(match formato {
                Formato::Monosans => geo::ler_monosans(&texto),
                Formato::Proxifly => geo::ler_proxifly(&texto),
                Formato::Texto => geo::ler_texto(&texto),
            }),
            _ => erros += 1,
        }
    }
    let todos = mesclar(lidos);
    if todos.is_empty() {
        anyhow::bail!(
            "não consegui baixar as listas de proxies ({erros} de {} falharam)",
            FONTES.len()
        );
    }
    Ok(todos)
}

/// Junta duplicados (mesmo ip:porta em várias listas), ficando com a melhor
/// informação geográfica: coordenada > país > nada.
fn mesclar(lidos: Vec<CandidatoGeo>) -> Vec<CandidatoGeo> {
    let qualidade = |c: &CandidatoGeo| c.coord.is_some() as u8 * 2 + c.pais.is_some() as u8;
    let mut por_chave: HashMap<(String, u16), CandidatoGeo> = HashMap::new();
    for c in lidos {
        let chave = (c.host.clone(), c.porta);
        match por_chave.get(&chave) {
            Some(atual) if qualidade(atual) >= qualidade(&c) => {}
            _ => {
                por_chave.insert(chave, c);
            }
        }
    }
    por_chave.into_values().collect()
}

fn embaralhar<T>(v: &mut [T]) {
    // xorshift semeado pelo relógio: suficiente para variar a ordem dos testes.
    let mut x = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
        | 1;
    for i in (1..v.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.swap(i, (x % (i as u64 + 1)) as usize);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_lista_e_descarta_lixo() {
        let t = "1.2.3.4:1080\r\nsocks5://5.6.7.8:9050\n10.0.0.1:1080\n127.0.0.1:1\nlixo\n9.9.9.9:0\n8.8.8.8:abc\n200.1.1.1:443 \n";
        let v: Vec<_> = geo::ler_texto(t).into_iter().map(|c| (c.host, c.porta)).collect();
        assert_eq!(
            v,
            vec![
                ("1.2.3.4".to_string(), 1080),
                ("5.6.7.8".to_string(), 9050),
                ("200.1.1.1".to_string(), 443)
            ]
        );
    }

    #[test]
    fn mesclar_fica_com_a_melhor_geografia() {
        let sem = CandidatoGeo { host: "1.1.1.1".into(), porta: 1, pais: None, coord: None };
        let com = CandidatoGeo { host: "1.1.1.1".into(), porta: 1, pais: Some("CL".into()), coord: Some((1.0, 2.0)) };
        let so_pais = CandidatoGeo { host: "1.1.1.1".into(), porta: 1, pais: Some("CL".into()), coord: None };
        let v = mesclar(vec![sem, com.clone(), so_pais]);
        assert_eq!(v, vec![com]);
    }

    fn saida(id: &str, ping: u32) -> Saida {
        Saida {
            id: id.into(),
            nome: id.into(),
            tipo: Tipo::Socks5,
            host: id.into(),
            porta: 1,
            usuario: None,
            senha: None,
            teste: Some(ResultadoTeste { ping_ms: Some(ping), ..Default::default() }),
        }
    }

    #[test]
    fn ordena_por_latencia_e_elimina_na_segunda_falha() {
        let p = Piscina::nova();
        p.inserir(saida("b", 300));
        p.inserir(saida("a", 100));
        p.inserir(saida("c", 200));
        let ids: Vec<_> = p.melhores(3).into_iter().map(|s| s.id).collect();
        assert_eq!(ids, ["a", "c", "b"]);

        assert!(p.promover("b"));
        assert_eq!(p.melhores(1)[0].id, "b");
        assert!(!p.promover("nao-existe"));
        p.falhou("b"); // volta para o fim
        assert_eq!(p.melhores(1)[0].id, "a");

        p.falhou("a"); // rebaixado para o fim
        assert_eq!(p.melhores(1)[0].id, "c");
        assert_eq!(p.tamanho(), 3);
        p.falhou("a"); // segunda falha: sai
        assert_eq!(p.tamanho(), 2);
    }

    /// Usa a internet de verdade: `cargo test piscina_real -- --ignored --nocapture`.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn piscina_real() {
        let p = Arc::new(Piscina::nova());
        let t0 = Instant::now();
        let r = p.atualizar().await;
        let e = p.estado();
        println!(
            "resultado={r:?} local={:?} candidatos={} testados={} saudaveis={} em {:?}",
            e.local,
            e.candidatos,
            e.testados,
            e.saudaveis,
            t0.elapsed()
        );
        for s in p.melhores(8) {
            let t = s.teste.unwrap();
            println!("  {:?} {}:{}  regiao={:?}  ping={:?}ms", t.pais, s.host, s.porta, t.regiao, t.ping_ms);
        }
    }

    #[test]
    fn embaralhar_mantem_os_itens() {
        let mut v: Vec<u32> = (0..100).collect();
        embaralhar(&mut v);
        let mut o = v.clone();
        o.sort();
        assert_eq!(o, (0..100).collect::<Vec<_>>());
    }
}
