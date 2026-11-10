import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Estado, ResultadoTeste, Saida } from "./tipos";

export const api = {
  estado: () => invoke<Estado>("estado"),
  adicionarSaida: (endereco: string, nome?: string) =>
    invoke<Estado>("adicionar_saida", { endereco, nome: nome || null }),
  renomearSaida: (id: string, nome: string) => invoke<Estado>("renomear_saida", { id, nome }),
  removerSaida: (id: string) => invoke<Estado>("remover_saida", { id }),
  escolherSaida: (id: string) => invoke<Estado>("escolher_saida", { id }),
  testarSaida: (id: string) => invoke<Estado>("testar_saida", { id }),
  testarDireto: () => invoke<ResultadoTeste>("testar_direto"),
  atualizarPiscina: () => invoke<Estado>("atualizar_piscina"),
  usarProxyPublico: (id: string) => invoke<Estado>("usar_proxy_publico", { id }),
  abrirGithub: () => invoke<void>("abrir_github"),
  reiniciarDiscord: () => invoke<Estado>("reiniciar_discord"),
  pausar: (pausado: boolean) => invoke<Estado>("pausar", { pausado }),
  definirOpcoes: (o: {
    estrito?: boolean;
    manterPelaSaida?: boolean;
    reabrirDiscordNoInicio?: boolean;
  }) =>
    invoke<Estado>("definir_opcoes", {
      estrito: o.estrito ?? null,
      manterPelaSaida: o.manterPelaSaida ?? null,
      reabrirDiscordNoInicio: o.reabrirDiscordNoInicio ?? null,
    }),
  exportarConvite: (id?: string) => invoke<string>("exportar_convite", { id: id ?? null }),
  importarConvite: (texto: string) => invoke<Estado>("importar_convite", { texto }),
  autostartAtivo: () => invoke<boolean>("autostart_ativo"),
  definirAutostart: (ativo: boolean) => invoke<boolean>("definir_autostart", { ativo }),
  sair: () => invoke<void>("sair"),
  aoMudarEstado: (f: (e: Estado) => void): Promise<UnlistenFn> =>
    listen<Estado>("estado", (ev) => f(ev.payload)),
  aoTrocarProxy: (f: (t: { de: Saida; para: Saida | null }) => void): Promise<UnlistenFn> =>
    listen<{ de: Saida; para: Saida | null }>("troca_de_proxy", (ev) => f(ev.payload)),
};

export function erroTexto(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
