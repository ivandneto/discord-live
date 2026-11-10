// Dados falsos para ver a interface no navegador (npm run dev), fora do Tauri.
// Só é carregado em modo de desenvolvimento. IPs dos blocos reservados para
// documentação (RFC 5737), para os prints públicos não mostrarem endereços reais.
import type { Estado, Saida } from "./tipos";

const agora = Date.now();

function publico(id: string, host: string, porta: number, pais: string, regiao: string, ping: number): Saida {
  return {
    id,
    nome: `Pública · ${regiao}`,
    tipo: "socks5",
    host,
    porta,
    usuario: null,
    tem_senha: false,
    teste: { ok: true, quando: agora - 40000, ping_ms: ping, ip: host, pais, regiao, erro: null },
  };
}

const membros: Saida[] = [
  publico("p1", "203.0.113.24", 1080, "US", "us-east", 412),
  publico("p2", "198.51.100.7", 3000, "AR", "buenos-aires", 472),
  publico("p3", "203.0.113.88", 1080, "US", "us-east", 663),
  publico("p4", "192.0.2.41", 44499, "US", "us-south", 671),
  publico("p5", "198.51.100.150", 1080, "US", "atlanta", 745),
  publico("p6", "203.0.113.201", 9080, "GB", "london", 1084),
  publico("p7", "192.0.2.77", 1088, "IT", "milan", 1773),
];
const emUso = membros[0];

export const estadoSimulado: Estado = {
  versao: "0.2.0",
  sessao: { fase: "abertura", correcao: "corrigida", saida_usada: emUso.nome, segundos_na_abertura: 0 },
  pausado: false,
  discord_aberto: true,
  discord_instalado: true,
  pac_ativo: true,
  porta_socks: 9350,
  saidas: [
    {
      id: "s1",
      nome: "VPS Chile",
      tipo: "socks5",
      host: "198.51.100.20",
      porta: 1080,
      usuario: "amigos",
      tem_senha: true,
      teste: { ok: true, quando: agora - 3600000, ping_ms: 61, ip: "198.51.100.20", pais: "CL", regiao: "santiago", erro: null },
    },
  ],
  escolhida: "auto",
  em_uso: emUso,
  coord_saida: [37.5, -78.0],
  auto: true,
  piscina: {
    atualizando: false,
    saudaveis: membros.length,
    candidatos: 2000,
    testados: 64,
    ultima_atualizacao: agora - 120000,
    erro: null,
    em_uso: emUso,
    local: "BR · GRU",
    local_coord: [-23.43, -46.47],
    membros,
  },
  estrito: true,
  manter_pela_saida: true,
  reabrir_discord_no_inicio: true,
  atividade: [
    { quando: agora - 2000, tipo: "info", host: "", porta: 0, descricao: "Sessão aberta pela saída Pública · us-east", detalhe: null },
    { quando: agora - 30000, tipo: "exterior", host: "gateway.discord.gg", porta: 443, descricao: "Conexão principal (gateway)", detalhe: emUso.nome },
    { quando: agora - 32000, tipo: "exterior", host: "discord.com", porta: 443, descricao: "API do Discord", detalhe: emUso.nome },
    { quando: agora - 33000, tipo: "exterior", host: "latency.discord.media", porta: 443, descricao: "Escolha de região", detalhe: emUso.nome },
    { quando: agora - 35000, tipo: "info", host: "", porta: 0, descricao: "Discord abriu", detalhe: null },
    { quando: agora - 60000, tipo: "info", host: "", porta: 0, descricao: "7 proxies funcionando fora do Brasil", detalhe: null },
  ],
  contagem_exterior: 3,
  contagem_direto: 0,
  contagem_bloqueado: 0,
  aviso: null,
};
