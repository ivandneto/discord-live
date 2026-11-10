import { useCallback, useEffect, useRef, useState } from "react";
import { api, erroTexto } from "./api";
import type { Atividade, Estado, Saida } from "./tipos";
import { haQuanto, nomePais, nomeRegiao } from "./util";
import { Globo } from "./Globo";

type Tom = "verde" | "azul" | "roxo" | "ambar" | "vermelho" | "cinza";
type Painel = null | "saidas" | "atividade" | "ajustes";
type Ordem = "melhores" | "ping";

interface Situacao {
  tom: Tom;
  titulo: string;
  texto: string;
  animado?: boolean;
}

function situacao(e: Estado): Situacao {
  if (e.aviso) return { tom: "vermelho", titulo: "Algo não subiu", texto: e.aviso };
  if (e.pausado) return { tom: "cinza", titulo: "Pausado", texto: "O Discord abre direto, sem correção." };
  if (!e.discord_instalado) return { tom: "ambar", titulo: "Discord não encontrado", texto: "Instale o Discord para PC." };
  if (e.auto && !e.em_uso) {
    return e.piscina.atualizando
      ? { tom: "roxo", titulo: "Buscando proxies…", texto: `Testando listas públicas · ${e.piscina.testados} testados`, animado: true }
      : { tom: "vermelho", titulo: "Sem proxy agora", texto: "Nenhum proxy público passou no teste. Tentando de novo…" };
  }
  if (!e.auto && !e.em_uso) return { tom: "ambar", titulo: "Escolha uma saída", texto: "Toque em Saídas para escolher." };

  const s = e.sessao;
  switch (s.correcao) {
    case "corrigida":
      return { tom: "verde", titulo: "Funcionando", texto: "Tela e câmera liberadas." };
    case "abrindo":
      return {
        tom: "roxo",
        titulo: "Abrindo pela saída…",
        texto: s.segundos_na_abertura ? `${s.segundos_na_abertura} s` : "Aguardando o Discord",
        animado: true,
      };
    case "aguardando":
      return { tom: "azul", titulo: "Pronto", texto: "Abra o Discord por aqui." };
    case "bloqueada":
      return { tom: "vermelho", titulo: "Saída fora do ar", texto: "Segurando o Discord até a saída responder." };
    case "sem_correcao":
      return { tom: "vermelho", titulo: "Sem correção", texto: "Reinicie o Discord." };
    default:
      return { tom: "ambar", titulo: "Reinicie o Discord", texto: "Ele abriu antes do Live Ponte." };
  }
}

function useAgora(ms = 1000) {
  const [agora, setAgora] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setAgora(Date.now()), ms);
    return () => clearInterval(t);
  }, [ms]);
  return agora;
}

export default function App() {
  const [estado, setEstado] = useState<Estado | null>(null);
  const [painel, setPainel] = useState<Painel>(null);
  const [ocupado, setOcupado] = useState<string | null>(null);
  const [aviso, setAviso] = useState<{ texto: string; erro?: boolean } | null>(null);
  const [alerta, setAlerta] = useState<{ titulo: string; texto: string; grave?: boolean } | null>(null);
  const avisoTimer = useRef<number | undefined>(undefined);
  const [ordem, setOrdem] = useState<Ordem>(() => {
    try {
      return localStorage.getItem("ordem-saidas") === "ping" ? "ping" : "melhores";
    } catch {
      return "melhores";
    }
  });
  const trocarOrdem = (o: Ordem) => {
    setOrdem(o);
    try {
      localStorage.setItem("ordem-saidas", o);
    } catch {
      /* sem armazenamento: só não lembra a escolha */
    }
  };

  const mostrar = useCallback((texto: string, erro = false) => {
    setAviso({ texto, erro });
    window.clearTimeout(avisoTimer.current);
    avisoTimer.current = window.setTimeout(() => setAviso(null), erro ? 5500 : 3000);
  }, []);

  useEffect(() => {
    if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
      import("./simulado").then((m) => setEstado(m.estadoSimulado));
      if (location.search.includes("alerta")) {
        setAlerta({
          titulo: "Proxy trocado automaticamente",
          texto:
            "O proxy de Buenos Aires parou de responder. Isso é normal: proxies públicos caem com frequência. Passei para EUA (Leste) (663 ms), o próximo melhor da lista. Você não precisa fazer nada.",
        });
      }
      return;
    }
    api.estado().then(setEstado).catch((e) => mostrar(erroTexto(e), true));
    const p = api.aoMudarEstado(setEstado);
    const q = api.aoTrocarProxy(({ de, para }) => {
      const nomeDe = nomeRegiao(de.teste?.regiao);
      setAlerta(
        para
          ? {
              titulo: "Proxy trocado automaticamente",
              texto: `O proxy de ${nomeDe} parou de responder. Isso é normal: proxies públicos caem com frequência. Passei para ${nomeRegiao(para.teste?.regiao)}${para.teste?.ping_ms != null ? ` (${para.teste.ping_ms} ms)` : ""}, o próximo melhor da lista. Você não precisa fazer nada.`,
            }
          : {
              titulo: "Proxy fora do ar",
              texto: `O proxy de ${nomeDe} parou de responder e nenhum outro respondeu agora. Estou buscando novos; câmera e tela podem falhar por alguns segundos.`,
              grave: true,
            },
      );
    });
    return () => {
      p.then((f) => f());
      q.then((f) => f());
    };
  }, [mostrar]);

  useEffect(() => {
    if (!alerta) return;
    const t = window.setTimeout(() => setAlerta(null), 12000);
    return () => window.clearTimeout(t);
  }, [alerta]);

  const executar = useCallback(
    async (chave: string, f: () => Promise<Estado | void>, ok?: string) => {
      setOcupado(chave);
      try {
        const r = await f();
        if (r) setEstado(r);
        if (ok) mostrar(ok);
      } catch (e) {
        mostrar(erroTexto(e), true);
      } finally {
        setOcupado(null);
      }
    },
    [mostrar],
  );

  if (!estado) {
    return (
      <div className="carregando">
        <div className="orbe orbe-roxo animado" />
      </div>
    );
  }

  const sit = situacao(estado);
  const uso = estado.em_uso;
  const t = uso?.teste;

  return (
    <div className="app">
      <header className="topo">
        <div className="marca">
          <Logo />
          <span>Live Ponte</span>
        </div>
        <div className="topo-icones">
          <BotaoIcone titulo="GitHub" onClick={() => api.abrirGithub().catch((e) => mostrar(erroTexto(e), true))}>
            <IconeGithub />
          </BotaoIcone>
          <BotaoIcone titulo="Ajustes" onClick={() => setPainel("ajustes")}>
            <IconeAjustes />
          </BotaoIcone>
        </div>
      </header>

      <main className="centro">
        <Globo voce={estado.piscina.local_coord} saida={estado.coord_saida} tom={sit.tom} />
        <div className="rota-legenda">
          <span>
            <i className="marca-voce" /> Você{estado.piscina.local ? ` · ${estado.piscina.local}` : ""}
          </span>
          {uso && (
            <span>
              <i className="marca-saida" /> {t?.regiao ? nomeRegiao(t.regiao) : uso.host}
            </span>
          )}
        </div>
        <h1 className="titulo-status">
          <i className={`ponto ponto-${sit.tom} ${sit.animado ? "animado" : ""}`} />
          {sit.titulo}
        </h1>
        <p className="sutil">{sit.texto}</p>

        <button className="saida-atual" onClick={() => setPainel("saidas")} title="Trocar saída">
          <Pais codigo={t?.pais} />
          <span className="saida-atual-texto">
            <b>{estado.auto ? "Automática" : uso?.nome ?? "Nenhuma saída"}</b>
            <small>
              {uso
                ? [t?.regiao ? nomeRegiao(t.regiao) : null, t?.ping_ms != null ? `${t.ping_ms} ms` : null]
                    .filter(Boolean)
                    .join(" · ") || uso.host
                : estado.auto
                  ? `${estado.piscina.saudaveis} proxies prontos`
                  : "toque para escolher"}
            </small>
          </span>
          <IconeSeta />
        </button>
      </main>

      <div className="acoes">
        <button
          className="botao primario grande"
          disabled={!!ocupado || !estado.discord_instalado}
          onClick={() => executar("reiniciar", api.reiniciarDiscord)}
        >
          {ocupado === "reiniciar" && <Giro />}
          {estado.discord_aberto ? "Reiniciar Discord" : "Abrir Discord"}
        </button>
        <BotaoIcone
          grande
          titulo={estado.pausado ? "Retomar" : "Pausar"}
          ativo={estado.pausado}
          onClick={() => executar("pausar", () => api.pausar(!estado.pausado))}
        >
          {estado.pausado ? <IconePlay /> : <IconePausa />}
        </BotaoIcone>
      </div>

      <nav className="abas">
        <button onClick={() => setPainel("saidas")}>
          <IconeGlobo />
          <span>Saídas</span>
        </button>
        <button onClick={() => setPainel("atividade")}>
          <IconeLista />
          <span>Atividade</span>
          {estado.contagem_bloqueado > 0 && <i className="ponto-alerta" />}
        </button>
        <span className={`estado-discord ${estado.discord_aberto ? "on" : ""}`}>
          <i /> Discord {estado.discord_aberto ? "aberto" : "fechado"}
        </span>
      </nav>

      {painel === "saidas" && (
        <Folha
          titulo="Saídas"
          aoFechar={() => setPainel(null)}
          acoes={
            <div className="segmentado" role="group" aria-label="Ordenar proxies">
              <button className={ordem === "melhores" ? "ativo" : ""} onClick={() => trocarOrdem("melhores")} title="Na ordem em que serão usados">
                Melhores
              </button>
              <button className={ordem === "ping" ? "ativo" : ""} onClick={() => trocarOrdem("ping")} title="Do menor ping ao maior">
                Ping
              </button>
            </div>
          }
        >
          <PainelSaidas estado={estado} ocupado={ocupado} executar={executar} mostrar={mostrar} ordem={ordem} />
        </Folha>
      )}
      {painel === "atividade" && (
        <Folha titulo="Atividade" aoFechar={() => setPainel(null)}>
          <ListaAtividade itens={estado.atividade} />
        </Folha>
      )}
      {painel === "ajustes" && (
        <Folha titulo="Ajustes" aoFechar={() => setPainel(null)}>
          <PainelAjustes estado={estado} executar={executar} mostrar={mostrar} />
        </Folha>
      )}

      {alerta && (
        <div className={`alerta ${alerta.grave ? "grave" : ""}`} role="alert">
          <IconeTroca />
          <div>
            <b>{alerta.titulo}</b>
            <p>{alerta.texto}</p>
          </div>
          <BotaoIcone titulo="Fechar" onClick={() => setAlerta(null)}>
            <IconeX />
          </BotaoIcone>
        </div>
      )}

      {aviso && <div className={`toast ${aviso.erro ? "erro" : ""}`}>{aviso.texto}</div>}
    </div>
  );
}

// --- Painéis -----------------------------------------------------------------

type Executar = (chave: string, f: () => Promise<Estado | void>, ok?: string) => Promise<void>;

function Folha(p: { titulo: string; aoFechar: () => void; acoes?: React.ReactNode; children: React.ReactNode }) {
  useEffect(() => {
    const f = (e: KeyboardEvent) => e.key === "Escape" && p.aoFechar();
    window.addEventListener("keydown", f);
    return () => window.removeEventListener("keydown", f);
  }, [p]);
  return (
    <div className="veu" onClick={p.aoFechar}>
      <section className="folha" onClick={(e) => e.stopPropagation()}>
        <header className="folha-topo">
          <h2>{p.titulo}</h2>
          {p.acoes && <div className="folha-acoes">{p.acoes}</div>}
          <BotaoIcone titulo="Fechar" onClick={p.aoFechar}>
            <IconeX />
          </BotaoIcone>
        </header>
        <div className="folha-corpo">{p.children}</div>
      </section>
    </div>
  );
}

function PainelSaidas(p: {
  estado: Estado;
  ocupado: string | null;
  executar: Executar;
  mostrar: (t: string, erro?: boolean) => void;
  ordem: Ordem;
}) {
  const { estado, executar } = p;
  const [form, setForm] = useState<null | "adicionar" | "importar">(null);
  const pis = estado.piscina;
  const idEmUso = estado.auto ? pis.em_uso?.id : undefined;
  const membros =
    p.ordem === "ping"
      ? [...pis.membros].sort((a, b) => (a.teste?.ping_ms ?? Infinity) - (b.teste?.ping_ms ?? Infinity))
      : pis.membros;

  return (
    <>
      <ul className="lista-saidas">
        <li className={`item-saida ${estado.auto ? "escolhida" : ""}`}>
          <button className="radio" disabled={estado.auto} onClick={() => executar("escolher", () => api.escolherSaida("auto"))}>
            <i />
          </button>
          <div className="item-corpo" onClick={() => !estado.auto && executar("escolher", () => api.escolherSaida("auto"))}>
            <b>Automática</b>
            <small className="sutil">
              {pis.atualizando
                ? `buscando… ${pis.testados}/${pis.candidatos}`
                : `${pis.saudaveis} proxies públicos prontos${pis.ultima_atualizacao ? ` · ${haQuanto(pis.ultima_atualizacao)}` : ""}`}
            </small>
          </div>
          <BotaoIcone titulo="Buscar de novo" onClick={() => executar("piscina", api.atualizarPiscina)}>
            {pis.atualizando ? <Giro /> : <IconeRecarregar />}
          </BotaoIcone>
        </li>

        {estado.auto && membros.length > 0 && (
          <li className="explica sutil">
            O proxy <b>em uso</b> é sempre o melhor disponível. Se ele cair, o próximo da lista assume sozinho e você
            recebe um aviso.
          </li>
        )}
        {membros.length > 0 && (
          <li className="sublista">
            <ul>
              {membros.map((m) => {
                const emUso = m.id === idEmUso;
                return (
                  <li key={m.id}>
                    <button
                      className={`proxy-publico ${emUso ? "em-uso" : ""}`}
                      disabled={emUso || !!p.ocupado}
                      title={emUso ? "Em uso" : "Usar este proxy"}
                      onClick={() => executar(`usar:${m.id}`, () => api.usarProxyPublico(m.id), "Proxy escolhido. Reinicie o Discord.")}
                    >
                      <span className="regiao">{nomeRegiao(m.teste?.regiao)}</span>
                      <span className="endereco sutil">
                        {m.host}:{m.porta}
                      </span>
                      <span className="ping">{m.teste?.ping_ms != null ? `${m.teste.ping_ms} ms` : ""}</span>
                      {emUso ? <span className="selo ok">em uso</span> : <span className="usar">usar</span>}
                    </button>
                  </li>
                );
              })}
            </ul>
          </li>
        )}

        {estado.saidas.map((s) => (
          <ItemSaida key={s.id} s={s} escolhida={!estado.auto && s.id === estado.escolhida} {...p} />
        ))}
      </ul>

      {form === null && (
        <div className="linha-botoes">
          <button className="botao pequeno" onClick={() => setForm("adicionar")}>
            + Saída própria
          </button>
          <button className="botao pequeno" onClick={() => setForm("importar")}>
            Importar convite
          </button>
        </div>
      )}
      {form === "adicionar" && (
        <FormTexto
          rotulo="Endereço do proxy"
          dica="socks5://usuario:senha@ip:1080"
          extra="Nome (opcional)"
          ocupado={p.ocupado === "adicionar"}
          aoCancelar={() => setForm(null)}
          aoEnviar={(end, nome) =>
            executar(
              "adicionar",
              async () => {
                const r = await api.adicionarSaida(end, nome);
                setForm(null);
                return r;
              },
              "Saída adicionada. Toque em testar.",
            )
          }
        />
      )}
      {form === "importar" && (
        <FormTexto
          rotulo="Convite ou lista de proxies"
          dica="ponte1.…"
          ocupado={p.ocupado === "importar"}
          aoCancelar={() => setForm(null)}
          aoEnviar={(texto) =>
            executar(
              "importar",
              async () => {
                const r = await api.importarConvite(texto);
                setForm(null);
                return r;
              },
              "Convite importado.",
            )
          }
        />
      )}
      <p className="nota sutil">
        A automática usa proxies públicos: veem que você usa o Discord, mas não o conteúdo. Para mais segurança, use uma
        saída própria.
      </p>
    </>
  );
}

function ItemSaida(p: {
  s: Saida;
  escolhida: boolean;
  ocupado: string | null;
  executar: Executar;
  mostrar: (t: string, erro?: boolean) => void;
}) {
  const { s, executar } = p;
  const t = s.teste;
  const testando = p.ocupado === `testar:${s.id}`;
  const [menu, setMenu] = useState(false);

  let selo: { classe: string; texto: string } | null = null;
  if (testando) selo = { classe: "neutro", texto: "testando" };
  else if (t?.ok) selo = { classe: "ok", texto: nomeRegiao(t.regiao) };
  else if (t?.regiao) selo = { classe: "falha", texto: "Brasil" };
  else if (t) selo = { classe: "falha", texto: "fora do ar" };

  return (
    <li className={`item-saida ${p.escolhida ? "escolhida" : ""}`}>
      <button className="radio" disabled={p.escolhida} onClick={() => executar("escolher", () => api.escolherSaida(s.id))}>
        <i />
      </button>
      <div className="item-corpo" title={t?.erro ?? undefined}>
        <b>
          {s.nome} {selo && <span className={`selo ${selo.classe}`}>{selo.texto}</span>}
        </b>
        <small className="sutil">
          {s.host}:{s.porta}
          {t?.pais ? ` · ${nomePais(t.pais)}` : ""}
          {t?.ping_ms != null ? ` · ${t.ping_ms} ms` : ""}
        </small>
      </div>
      <BotaoIcone titulo="Testar" onClick={() => executar(`testar:${s.id}`, () => api.testarSaida(s.id))}>
        {testando ? <Giro /> : <IconeRaio />}
      </BotaoIcone>
      <div className="menu-ancora">
        <BotaoIcone titulo="Mais" onClick={() => setMenu(!menu)}>
          <IconeMais />
        </BotaoIcone>
        {menu && (
          <div className="menu" onMouseLeave={() => setMenu(false)}>
            <button
              onClick={async () => {
                setMenu(false);
                try {
                  await navigator.clipboard.writeText(await api.exportarConvite(s.id));
                  p.mostrar("Convite copiado (inclui a senha).");
                } catch (e) {
                  p.mostrar(erroTexto(e), true);
                }
              }}
            >
              Copiar convite
            </button>
            <button
              onClick={() => {
                setMenu(false);
                const nome = prompt("Novo nome", s.nome);
                if (nome) executar("renomear", () => api.renomearSaida(s.id, nome));
              }}
            >
              Renomear
            </button>
            <button
              className="perigo"
              onClick={() => {
                setMenu(false);
                if (confirm(`Remover "${s.nome}"?`)) executar("remover", () => api.removerSaida(s.id));
              }}
            >
              Remover
            </button>
          </div>
        )}
      </div>
    </li>
  );
}

function FormTexto(p: {
  rotulo: string;
  dica: string;
  extra?: string;
  ocupado: boolean;
  aoCancelar: () => void;
  aoEnviar: (a: string, b: string) => void;
}) {
  const [a, setA] = useState("");
  const [b, setB] = useState("");
  return (
    <form
      className="formulario"
      onSubmit={(e) => {
        e.preventDefault();
        if (a.trim()) p.aoEnviar(a.trim(), b.trim());
      }}
    >
      <input autoFocus placeholder={p.dica} aria-label={p.rotulo} value={a} onChange={(e) => setA(e.target.value)} spellCheck={false} />
      {p.extra && <input placeholder={p.extra} aria-label={p.extra} value={b} onChange={(e) => setB(e.target.value)} />}
      <div className="linha-botoes fim">
        <button type="button" className="botao pequeno" onClick={p.aoCancelar}>
          Cancelar
        </button>
        <button type="submit" className="botao pequeno primario" disabled={p.ocupado || !a.trim()}>
          {p.ocupado && <Giro />}OK
        </button>
      </div>
    </form>
  );
}

function PainelAjustes(p: { estado: Estado; executar: Executar; mostrar: (t: string, erro?: boolean) => void }) {
  const { estado, executar } = p;
  const [autostart, setAutostart] = useState<boolean | null>(null);
  useEffect(() => {
    api.autostartAtivo().then(setAutostart).catch(() => setAutostart(false));
  }, []);

  return (
    <>
      <Interruptor
        rotulo="Iniciar com o PC"
        ligado={!!autostart}
        aoMudar={async (v) => {
          try {
            setAutostart(await api.definirAutostart(v));
          } catch (e) {
            p.mostrar(erroTexto(e), true);
          }
        }}
      />
      <Interruptor
        rotulo="Modo estrito"
        dica="Se a saída falhar, segura o Discord em vez de abrir pelo Brasil."
        ligado={estado.estrito}
        aoMudar={(v) => executar("opc", () => api.definirOpcoes({ estrito: v }))}
      />
      <Interruptor
        rotulo="Sempre pela saída"
        dica="Necessário para câmera e tela: o Discord confere o IP de novo ao ligar cada uma."
        ligado={estado.manter_pela_saida}
        aoMudar={(v) => executar("opc", () => api.definirOpcoes({ manterPelaSaida: v }))}
      />
      <Interruptor
        rotulo="Reabrir o Discord ao ligar"
        dica="Se ele abrir antes do Live Ponte."
        ligado={estado.reabrir_discord_no_inicio}
        aoMudar={(v) => executar("opc", () => api.definirOpcoes({ reabrirDiscordNoInicio: v }))}
      />
      <div className="rodape-ajustes">
        <span className="sutil">
          v{estado.versao} · porta {estado.porta_socks || "—"} · proxy automático {estado.pac_ativo ? "ligado" : "desligado"}
        </span>
        <button className="link perigo" onClick={() => api.sair()}>
          Sair do Live Ponte
        </button>
      </div>
    </>
  );
}

const ROTULO: Record<Atividade["tipo"], string> = {
  exterior: "saída",
  direto: "direto",
  bloqueado: "bloq.",
  info: "info",
  aviso: "aviso",
};

function ListaAtividade({ itens }: { itens: Atividade[] }) {
  const agora = useAgora(5000);
  if (itens.length === 0) return <p className="sutil vazio">Nada ainda.</p>;
  return (
    <ul className="atividade">
      {itens.map((a, i) => (
        <li key={`${a.quando}-${i}`} className={`at at-${a.tipo}`} title={a.detalhe ?? a.host}>
          <span className="at-tipo">{ROTULO[a.tipo]}</span>
          <span className="at-desc">{a.descricao}</span>
          <span className="at-quando sutil">{haQuanto(a.quando, agora)}</span>
        </li>
      ))}
    </ul>
  );
}

// --- Peças --------------------------------------------------------------------

function Interruptor(p: { rotulo: string; dica?: string; ligado: boolean; aoMudar: (v: boolean) => void }) {
  return (
    <div className="interruptor" onClick={() => p.aoMudar(!p.ligado)}>
      <span>
        {p.rotulo}
        {p.dica && <small className="sutil">{p.dica}</small>}
      </span>
      <button type="button" role="switch" aria-checked={p.ligado} aria-label={p.rotulo} className={`chave ${p.ligado ? "ligada" : ""}`}>
        <i />
      </button>
    </div>
  );
}

function BotaoIcone(p: { titulo: string; onClick: () => void; children: React.ReactNode; grande?: boolean; ativo?: boolean }) {
  return (
    <button
      className={`icone ${p.grande ? "grande" : ""} ${p.ativo ? "ativo" : ""}`}
      title={p.titulo}
      aria-label={p.titulo}
      onClick={p.onClick}
    >
      {p.children}
    </button>
  );
}

function Giro() {
  return <span className="giro" aria-hidden />;
}

/** Selo com a sigla do país: o Windows não desenha bandeiras em emoji. */
function Pais({ codigo }: { codigo?: string | null }) {
  const ok = codigo && /^[A-Za-z]{2}$/.test(codigo) && codigo.toUpperCase() !== "XX";
  return (
    <span className="pais" title={ok ? nomePais(codigo) : "País desconhecido"}>
      {ok ? codigo!.toUpperCase() : <IconeGlobo />}
    </span>
  );
}

function Logo() {
  return (
    <svg className="logo" viewBox="0 0 1024 1024" aria-hidden>
      <defs>
        <linearGradient id="lg" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#6d5dfc" />
          <stop offset="1" stopColor="#2fb7a4" />
        </linearGradient>
      </defs>
      <rect x="32" y="32" width="960" height="960" rx="224" fill="url(#lg)" />
      <g fill="none" stroke="#fff" strokeLinecap="round">
        <circle cx="512" cy="604" r="250" strokeWidth="58" />
        <ellipse cx="512" cy="604" rx="104" ry="250" strokeWidth="44" />
        <path d="M262 604 H762" strokeWidth="44" />
      </g>
      <path d="M238 330 C 380 132, 668 128, 786 316" fill="none" stroke="#fff" strokeWidth="66" strokeLinecap="round" />
      <path d="M716 338 L 846 262 L 842 404 Z" fill="#fff" stroke="#fff" strokeWidth="30" strokeLinejoin="round" />
      <circle cx="238" cy="330" r="60" fill="#3fd197" stroke="#fff" strokeWidth="24" />
    </svg>
  );
}

const Svg = (p: { d: string; preenchido?: boolean }) => (
  <svg viewBox="0 0 24 24" className={p.preenchido ? "preenchido" : ""} aria-hidden>
    <path d={p.d} />
  </svg>
);
const IconeAjustes = () => (
  <Svg d="M4 7h10M18 7h2M4 17h4M12 17h8M16 5v4M10 15v4" />
);
const IconeGlobo = () => <Svg d="M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18M3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18" />;
const IconeLista = () => <Svg d="M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01" />;
const IconeSeta = () => <Svg d="M9 6l6 6-6 6" />;
const IconePausa = () => <Svg d="M9 6v12M15 6v12" />;
const IconePlay = () => <Svg d="M8 5l11 7-11 7z" />;
const IconeX = () => <Svg d="M6 6l12 12M18 6L6 18" />;
const IconeRaio = () => <Svg d="M13 3L5 14h6l-1 7 8-11h-6z" />;
const IconeMais = () => <Svg d="M12 6h.01M12 12h.01M12 18h.01" />;
const IconeRecarregar = () => <Svg d="M20 11a8 8 0 1 0-2.3 5.7M20 5v6h-6" />;
const IconeTroca = () => <Svg d="M4 8h13l-3-3M20 16H7l3 3" />;
const IconeGithub = () => (
  <Svg
    preenchido
    d="M12 2a10 10 0 0 0-3.2 19.5c.5.1.7-.2.7-.5v-1.7c-2.8.6-3.4-1.3-3.4-1.3-.5-1.2-1.1-1.5-1.1-1.5-.9-.6.1-.6.1-.6 1 .1 1.5 1 1.5 1 .9 1.5 2.4 1.1 3 .8.1-.6.4-1.1.6-1.3-2.2-.3-4.6-1.1-4.6-5 0-1.1.4-2 1-2.7-.1-.3-.4-1.3.1-2.7 0 0 .8-.3 2.8 1a9.6 9.6 0 0 1 5 0c1.9-1.3 2.8-1 2.8-1 .5 1.4.2 2.4.1 2.7.6.7 1 1.6 1 2.7 0 3.9-2.3 4.7-4.6 5 .4.3.7.9.7 1.9v2.8c0 .3.2.6.7.5A10 10 0 0 0 12 2z"
  />
);
