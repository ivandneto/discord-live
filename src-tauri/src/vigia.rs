//! Vigia do processo principal do Discord.
//!
//! O principal é o único `Discord.exe` cujo pai não é outro `Discord.exe`
//! (numa máquina comum são vários: um principal e os filhos). A identidade é
//! PID + hora de nascimento, porque o Windows reaproveita PIDs. Uma leitura
//! vazia isolada não conta como "fechou": são precisas três seguidas.

use crate::discord;
use std::collections::HashSet;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone)]
pub enum Evento {
    Abriu { exe: String },
    Fechou,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Identidade {
    pid: u32,
    nascimento: u64,
    exe: String,
}

pub struct Leitor {
    sys: System,
}

impl Leitor {
    pub fn novo() -> Self {
        Leitor { sys: System::new() }
    }

    fn principal(&mut self) -> Option<Identidade> {
        self.sys
            .refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
        let procs = self.sys.processes();
        let do_discord: HashSet<sysinfo::Pid> = procs
            .iter()
            .filter(|(_, p)| discord::e_executavel_do_discord(&p.name().to_string_lossy()))
            .map(|(pid, _)| *pid)
            .collect();
        procs
            .iter()
            .filter(|(pid, _)| do_discord.contains(pid))
            .filter(|(_, p)| !p.parent().is_some_and(|pai| do_discord.contains(&pai)))
            .map(|(pid, p)| Identidade {
                pid: pid.as_u32(),
                nascimento: p.start_time(),
                exe: p.name().to_string_lossy().into_owned(),
            })
            .min_by_key(|i| i.nascimento)
    }

    /// Para a leitura inicial, antes de o vigia começar.
    pub fn exe_aberto(&mut self) -> Option<String> {
        self.principal().map(|i| i.exe)
    }

    pub fn algum_aberto(&mut self) -> bool {
        self.principal().is_some()
    }
}

/// Roda numa thread própria. `inicial` é o que já estava aberto ao começar.
pub fn iniciar(inicial: Option<String>, tx: UnboundedSender<Evento>) {
    std::thread::Builder::new()
        .name("vigia-discord".into())
        .spawn(move || {
            let mut leitor = Leitor::novo();
            let mut atual: Option<Identidade> = if inicial.is_some() { leitor.principal() } else { None };
            let mut vazias = 0u32;
            loop {
                std::thread::sleep(Duration::from_millis(700));
                match leitor.principal() {
                    None => {
                        vazias += 1;
                        if vazias >= 3 && atual.is_some() {
                            atual = None;
                            if tx.send(Evento::Fechou).is_err() {
                                return;
                            }
                        }
                    }
                    Some(id) => {
                        vazias = 0;
                        if atual.as_ref() != Some(&id) {
                            let exe = id.exe.clone();
                            atual = Some(id);
                            if tx.send(Evento::Abriu { exe }).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        })
        .expect("thread do vigia");
}
