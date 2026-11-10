//! Comandos chamados pela interface.

use crate::nucleo::{Estado, Nucleo, TipoAtividade};
use crate::piscina::ID_AUTO;
use crate::saidas::{self, ResultadoTeste};
use crate::teste;
use std::sync::Arc;
use tauri::State;
use tauri_plugin_autostart::ManagerExt;

type N<'a> = State<'a, Arc<Nucleo>>;

fn pronto(n: &Nucleo) -> Estado {
    n.emitir();
    n.estado()
}

#[tauri::command]
pub fn estado(n: N<'_>) -> Estado {
    n.estado()
}

#[tauri::command]
pub fn adicionar_saida(n: N<'_>, endereco: String, nome: Option<String>) -> Result<Estado, String> {
    let nova = saidas::ler(&endereco, nome.as_deref()).map_err(|e| e.to_string())?;
    n.com_config(|c| {
        if c.saidas.iter().any(|s| s.mesmo_destino(&nova)) {
            return Err("essa saída já está na lista".to_string());
        }
        // Quem adiciona uma saída própria quer usá-la.
        c.escolhida = Some(nova.id.clone());
        c.saidas.push(nova);
        Ok(())
    })?;
    Ok(pronto(&n))
}

#[tauri::command]
pub fn renomear_saida(n: N<'_>, id: String, nome: String) -> Result<Estado, String> {
    let nome = nome.trim().to_string();
    if nome.is_empty() {
        return Err("o nome não pode ficar vazio".into());
    }
    n.com_config(|c| {
        if let Some(s) = c.saidas.iter_mut().find(|s| s.id == id) {
            s.nome = nome;
        }
    });
    Ok(pronto(&n))
}

#[tauri::command]
pub fn remover_saida(n: N<'_>, id: String) -> Estado {
    n.com_config(|c| {
        c.saidas.retain(|s| s.id != id);
        if c.escolhida.as_deref() == Some(id.as_str()) {
            c.escolhida = Some(ID_AUTO.to_string());
        }
    });
    pronto(&n)
}

#[tauri::command]
pub fn escolher_saida(n: N<'_>, id: String) -> Estado {
    let auto = id == ID_AUTO;
    n.com_config(|c| {
        if auto || c.saidas.iter().any(|s| s.id == id) {
            c.escolhida = Some(id);
        }
    });
    if auto && n.piscina.tamanho() == 0 {
        n.inner().atualizar_piscina();
    }
    n.info(TipoAtividade::Info, "Saída trocada: reinicie o Discord para usar a nova");
    pronto(&n)
}

#[tauri::command]
pub async fn testar_saida(n: N<'_>, id: String) -> Result<Estado, String> {
    let s = n.saida(&id).ok_or("saída não encontrada")?;
    let r = teste::testar(Some(&s)).await;
    n.com_config(|c| {
        if let Some(x) = c.saidas.iter_mut().find(|x| x.id == id) {
            x.teste = Some(r);
        }
    });
    Ok(pronto(&n))
}

#[tauri::command]
pub fn usar_proxy_publico(n: N<'_>, id: String) -> Result<Estado, String> {
    if !n.piscina.promover(&id) {
        return Err("esse proxy saiu da lista (caiu); escolha outro".into());
    }
    n.com_config(|c| c.escolhida = Some(ID_AUTO.to_string()));
    n.info(TipoAtividade::Info, "Proxy escolhido: reinicie o Discord para usar");
    Ok(pronto(&n))
}

#[tauri::command]
pub fn atualizar_piscina(n: N<'_>) -> Estado {
    n.inner().atualizar_piscina();
    pronto(&n)
}

pub const GITHUB: &str = "https://github.com/ivandneto/discord-live";

#[tauri::command]
pub fn abrir_github() -> Result<(), String> {
    // URL fixa no código: a interface não consegue mandar abrir outra coisa.
    tauri_plugin_opener::open_url(GITHUB, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn testar_direto() -> ResultadoTeste {
    teste::testar(None).await
}

#[tauri::command]
pub async fn reiniciar_discord(n: N<'_>) -> Result<Estado, String> {
    let nucleo = n.inner().clone();
    nucleo.reiniciar_discord().await?;
    Ok(pronto(&n))
}

#[tauri::command]
pub fn pausar(n: N<'_>, pausado: bool) -> Estado {
    n.com_config(|c| c.pausado = pausado);
    n.aplicar_pac();
    n.info(
        TipoAtividade::Info,
        if pausado {
            "Pausado: o Discord abre direto, sem correção"
        } else {
            "Retomado: reinicie o Discord para a sessão nascer pela saída"
        },
    );
    pronto(&n)
}

#[tauri::command]
pub fn definir_opcoes(
    n: N<'_>,
    estrito: Option<bool>,
    manter_pela_saida: Option<bool>,
    reabrir_discord_no_inicio: Option<bool>,
) -> Estado {
    n.com_config(|c| {
        if let Some(v) = estrito {
            c.estrito = v;
        }
        if let Some(v) = manter_pela_saida {
            c.manter_pela_saida = v;
        }
        if let Some(v) = reabrir_discord_no_inicio {
            c.reabrir_discord_no_inicio = v;
        }
    });
    pronto(&n)
}

#[tauri::command]
pub fn exportar_convite(n: N<'_>, id: Option<String>) -> Result<String, String> {
    if id.as_deref() == Some(ID_AUTO) || (id.is_none() && n.usando_auto()) {
        return Err("a saída automática não precisa de convite: cada um busca a própria lista".into());
    }
    let s = match id {
        Some(id) => n.saida(&id),
        None => n.saida_escolhida(),
    }
    .ok_or("escolha uma saída para compartilhar")?;
    Ok(saidas::exportar(&[&s]))
}

#[tauri::command]
pub fn importar_convite(n: N<'_>, texto: String) -> Result<Estado, String> {
    let novas = saidas::importar(&texto).map_err(|e| format!("{e:#}"))?;
    let mut adicionadas = 0;
    n.com_config(|c| {
        let mut primeira = None;
        for s in novas {
            if let Some(existente) = c.saidas.iter_mut().find(|x| x.mesmo_destino(&s)) {
                // Mesmo destino: atualiza senha e nome, mantém o histórico de teste.
                existente.senha = s.senha;
                existente.nome = s.nome;
                primeira.get_or_insert(existente.id.clone());
            } else {
                primeira.get_or_insert(s.id.clone());
                c.saidas.push(s);
                adicionadas += 1;
            }
        }
        if let Some(id) = primeira {
            c.escolhida = Some(id);
        }
    });
    n.info(
        TipoAtividade::Info,
        format!("Convite importado ({adicionadas} saída(s) nova(s)); teste e reinicie o Discord"),
    );
    Ok(pronto(&n))
}

#[tauri::command]
pub fn autostart_ativo(app: tauri::AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn definir_autostart(app: tauri::AppHandle, ativo: bool) -> Result<bool, String> {
    let al = app.autolaunch();
    if ativo {
        al.enable().map_err(|e| e.to_string())?;
    } else {
        al.disable().map_err(|e| e.to_string())?;
    }
    Ok(al.is_enabled().unwrap_or(false))
}

#[tauri::command]
pub fn sair(app: tauri::AppHandle, n: N<'_>) {
    n.encerrar();
    app.exit(0);
}
