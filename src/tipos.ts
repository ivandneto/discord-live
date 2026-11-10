export type Fase = "abertura" | "estabelecida";
export type Correcao =
  | "desconhecida"
  | "aguardando"
  | "abrindo"
  | "corrigida"
  | "sem_correcao"
  | "bloqueada";

export interface ResultadoTeste {
  ok: boolean;
  quando: number;
  ping_ms: number | null;
  ip: string | null;
  pais: string | null;
  regiao: string | null;
  erro: string | null;
}

export interface Saida {
  id: string;
  nome: string;
  tipo: "socks5" | "http";
  host: string;
  porta: number;
  usuario: string | null;
  tem_senha: boolean;
  teste: ResultadoTeste | null;
}

export type TipoAtividade = "exterior" | "direto" | "bloqueado" | "info" | "aviso";

export interface Atividade {
  quando: number;
  tipo: TipoAtividade;
  host: string;
  porta: number;
  descricao: string;
  detalhe: string | null;
}

export interface Estado {
  versao: string;
  sessao: {
    fase: Fase;
    correcao: Correcao;
    saida_usada: string | null;
    segundos_na_abertura: number;
  };
  pausado: boolean;
  discord_aberto: boolean;
  discord_instalado: boolean;
  pac_ativo: boolean;
  porta_socks: number;
  saidas: Saida[];
  escolhida: string | null;
  em_uso: Saida | null;
  coord_saida: [number, number] | null;
  auto: boolean;
  piscina: {
    atualizando: boolean;
    saudaveis: number;
    candidatos: number;
    testados: number;
    ultima_atualizacao: number | null;
    erro: string | null;
    em_uso: Saida | null;
    membros: Saida[];
    local: string | null;
    local_coord: [number, number] | null;
  };
  estrito: boolean;
  manter_pela_saida: boolean;
  reabrir_discord_no_inicio: boolean;
  atividade: Atividade[];
  contagem_exterior: number;
  contagem_direto: number;
  contagem_bloqueado: number;
  aviso: string | null;
}
