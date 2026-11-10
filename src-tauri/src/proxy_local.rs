//! Proxy SOCKS5 local em 127.0.0.1. É o único endereço que o Discord conhece
//! (pelo PAC). Para cada conexão, decide o caminho: saída estrangeira ou direto.

use crate::conector;
use crate::nucleo::{Nucleo, TipoAtividade};
use crate::rotas::{self, Rota};
use crate::sessao::Fase;
use anyhow::{bail, Result};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub async fn iniciar(n: Arc<Nucleo>, preferida: u16) -> Result<u16> {
    for p in preferida..preferida + 20 {
        if let Ok(escuta) = TcpListener::bind(("127.0.0.1", p)).await {
            tokio::spawn(aceitar(escuta, n));
            return Ok(p);
        }
    }
    bail!("nenhuma porta livre para o proxy local")
}

async fn aceitar(escuta: TcpListener, n: Arc<Nucleo>) {
    loop {
        match escuta.accept().await {
            Ok((c, _)) => {
                let n = n.clone();
                tokio::spawn(async move {
                    let _ = atender(c, n).await;
                });
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
}

async fn responder(c: &mut TcpStream, codigo: u8) -> Result<()> {
    c.write_all(&[5, codigo, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
    Ok(())
}

async fn aperto_de_mao(c: &mut TcpStream) -> Result<(String, u16)> {
    let mut cab = [0u8; 2];
    c.read_exact(&mut cab).await?;
    if cab[0] != 5 {
        bail!("não é SOCKS5");
    }
    let mut metodos = vec![0u8; cab[1] as usize];
    c.read_exact(&mut metodos).await?;
    if !metodos.contains(&0) {
        c.write_all(&[5, 0xFF]).await?;
        bail!("cliente sem método sem senha");
    }
    c.write_all(&[5, 0]).await?;

    let mut req = [0u8; 4];
    c.read_exact(&mut req).await?;
    if req[0] != 5 {
        bail!("pedido SOCKS5 malformado");
    }
    let host = match req[3] {
        1 => {
            let mut a = [0u8; 4];
            c.read_exact(&mut a).await?;
            Ipv4Addr::from(a).to_string()
        }
        3 => {
            let mut l = [0u8; 1];
            c.read_exact(&mut l).await?;
            let mut b = vec![0u8; l[0] as usize];
            c.read_exact(&mut b).await?;
            match String::from_utf8(b) {
                Ok(h) if rotas::nome_bem_formado(&h) => h,
                _ => {
                    responder(c, 1).await?;
                    bail!("nome de host malformado");
                }
            }
        }
        4 => {
            let mut a = [0u8; 16];
            c.read_exact(&mut a).await?;
            Ipv6Addr::from(a).to_string()
        }
        _ => {
            responder(c, 8).await?;
            bail!("tipo de endereço não suportado");
        }
    };
    let mut p = [0u8; 2];
    c.read_exact(&mut p).await?;
    let porta = u16::from_be_bytes(p);
    if req[1] != 1 {
        // Só CONNECT. UDP ASSOCIATE nunca é pedido: áudio e vídeo não passam por proxy.
        responder(c, 7).await?;
        bail!("comando SOCKS não suportado");
    }
    Ok((host, porta))
}

async fn atender(mut c: TcpStream, n: Arc<Nucleo>) -> Result<()> {
    let _ = c.set_nodelay(true);
    let (host, porta) = tokio::time::timeout(Duration::from_secs(10), aperto_de_mao(&mut c)).await??;

    // A rota depende de haver uma saída *configurada*, não de ela estar de pé:
    // com a piscina vazia, o modo estrito precisa segurar, não mandar direto.
    let configurada = !n.pausado() && n.saida_configurada();
    // Com "sempre pela saída", o Discord confere o IP também depois da abertura
    // (ao ligar a câmera): os domínios dele saem por fora em qualquer fase.
    let em_abertura = n.sessao.fase() == Fase::Abertura || n.manter_pela_saida();
    let rota = if configurada {
        rotas::decidir(&host, em_abertura)
    } else {
        Rota::Direta
    };
    let decisivo = rotas::decide_regiao(&host);

    if rota == Rota::Direta {
        // Conta também quando pausado: pausar no meio da abertura deixa a sessão sem correção.
        if em_abertura && decisivo {
            n.sessao.direta_decisiva(Instant::now());
        }
        let mut destino = match conector::direto(&host, porta).await {
            Ok(d) => d,
            Err(_) => {
                responder(&mut c, 4).await?;
                return Ok(());
            }
        };
        n.registrar(TipoAtividade::Direto, &host, porta, None);
        responder(&mut c, 0).await?;
        let _ = copy_bidirectional(&mut c, &mut destino).await;
        return Ok(());
    }

    // Rota pela saída estrangeira. Na automática, uma segunda tentativa noutro
    // proxy da piscina; quem falha é rebaixado.
    let auto = n.usando_auto();
    let mut r: Result<(TcpStream, crate::saidas::Saida)> =
        Err(anyhow::anyhow!("nenhum proxy disponível ainda (buscando nas listas públicas)"));
    // O proxy que estava em uso, se ele falhar nesta conexão (para avisar o usuário).
    let mut caiu: Option<crate::saidas::Saida> = None;
    for (i, s) in n.saidas_para_tentar().into_iter().enumerate() {
        n.sessao.handshake_inicio();
        let tentativa = conector::via(&s, &host, porta, conector::PRAZO_SAIDA).await;
        n.sessao.handshake_fim();
        match tentativa {
            Ok(d) => {
                if auto {
                    n.piscina.funcionou(&s.id);
                }
                r = Ok((d, s));
                break;
            }
            Err(e) => {
                if auto {
                    n.piscina.falhou(&s.id);
                    if i == 0 {
                        caiu = Some(s);
                    }
                }
                r = Err(e);
            }
        }
    }
    if let Some(de) = caiu {
        let para = r.as_ref().ok().map(|(_, s)| s.publica());
        n.alertar_troca(de.publica(), para);
    }

    match r {
        Ok((mut destino, s)) => {
            if decisivo {
                n.sessao.sucesso_decisivo(&s.nome, Instant::now());
            }
            n.registrar(TipoAtividade::Exterior, &host, porta, Some(s.nome.clone()));

            // Quando a janela de abertura fecha, estas conexões são derrubadas e o
            // Discord reconecta (RESUME, mesma sessão) pelo caminho direto.
            let derrubar = n.derrubar.notified();
            tokio::pin!(derrubar);
            derrubar.as_mut().enable();
            if n.sessao.fase() == Fase::Estabelecida && !n.manter_pela_saida() {
                return Ok(());
            }
            responder(&mut c, 0).await?;
            tokio::select! {
                _ = copy_bidirectional(&mut c, &mut destino) => {}
                _ = &mut derrubar => {}
            }
            Ok(())
        }
        Err(e) => {
            if n.estrito() {
                if decisivo {
                    n.sessao.falha_decisiva();
                }
                n.registrar(TipoAtividade::Bloqueado, &host, porta, Some(e.to_string()));
                responder(&mut c, 5).await?;
                return Ok(());
            }
            // Sem modo estrito: perde-se a correção, nunca a conexão.
            if decisivo {
                n.sessao.direta_decisiva(Instant::now());
            }
            n.registrar(TipoAtividade::Direto, &host, porta, Some(format!("saída falhou: {e}")));
            let mut destino = match conector::direto(&host, porta).await {
                Ok(d) => d,
                Err(_) => {
                    responder(&mut c, 4).await?;
                    return Ok(());
                }
            };
            responder(&mut c, 0).await?;
            let _ = copy_bidirectional(&mut c, &mut destino).await;
            Ok(())
        }
    }
}
